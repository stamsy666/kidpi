"""kidpi's tweaks for Flowseal's tg-ws-proxy, applied before the proxy starts.

kidpi starts the proxy with ``python -c ...`` and imports this module first.
It lives in kidpi's own folder, not next to the proxy code, so it stays when
kidpi updates tg-ws-proxy from GitHub. Each patch first checks that the code it
touches still looks as expected; if it does not, that patch is skipped and the
proxy runs exactly as upstream wrote it.

1. CF proxy fallback: Cloudflare domains are tried a few at a time (one more
   every STAGGER seconds while the earlier ones hang) instead of one after
   another with a 10-second timeout each. Telegram Desktop drops a connection
   that gets no answer for ~10 s, so with one or two dead domains in front the
   working one used to be found after Telegram had already left
   ("Switched active CF domain" followed by "WS session closed ... v0.0B in 0.0s").
2. Domains that just failed go to the end of the list for a while, for both the
   WS fallback and the H2 media lanes. Before, every new connection paid the
   timeout of the same dead domain again.
3. The direct WS pool waits at most 5 minutes between refill attempts instead
   of an hour, so after sleep or a network drop the fast route comes back.
"""

import asyncio
import inspect
import logging
import time

log = logging.getLogger('tg-mtproto-proxy')

# CF fallback race
STAGGER = 1.5          # start one more domain when nothing answered for this long
MAX_PARALLEL = 3       # attempts in flight at once
ATTEMPT_TIMEOUT = 8.0  # one domain: TCP + TLS, then the WebSocket upgrade answer
DEADLINE = 12.0        # the whole fallback; Telegram gives up about then anyway

# a failed domain goes last for 60 s, 120 s, ... up to 15 min; one success clears it
HEALTH_COOLDOWN = 60.0
HEALTH_COOLDOWN_MAX = 900.0
# an H2 preflight cancelled after hanging this long counts as a failure
H2_HANG_SECONDS = 5.0

WS_POOL_BACKOFF_MAX = 300.0

applied = []
skipped = []


class DomainHealth:
    """Remembers which CF proxy base domains failed lately; only reorders, never hides."""

    def __init__(self):
        self._fails = {}
        self._until = {}

    def failed(self, domain):
        n = self._fails.get(domain, 0) + 1
        self._fails[domain] = n
        cooldown = min(HEALTH_COOLDOWN * 2 ** min(n - 1, 10), HEALTH_COOLDOWN_MAX)
        self._until[domain] = time.monotonic() + cooldown

    def ok(self, domain):
        self._fails.pop(domain, None)
        self._until.pop(domain, None)

    def is_bad(self, domain, now=None):
        return self._until.get(domain, 0.0) > (time.monotonic() if now is None else now)

    def order(self, domains):
        """Healthy domains keep their order; failed ones follow, soonest-to-recover first."""
        now = time.monotonic()
        good = [d for d in domains if not self.is_bad(d, now)]
        bad = [d for d in domains if self.is_bad(d, now)]
        bad.sort(key=lambda d: self._until.get(d, 0.0))
        return good + bad


health = DomainHealth()


def _cf_base(balancer, host, domain):
    """'kws2.<base>' for a CF proxy connection (host is the domain itself), else None."""
    if not isinstance(host, str) or host != domain or not host.startswith('kws'):
        return None
    _, _, base = host.partition('.')
    return base if base in balancer.domains else None


def _patch_balancer(balancer_mod):
    cls = type(balancer_mod.balancer)
    orig = cls.get_domains_for_dc
    if list(inspect.signature(orig).parameters) != ['self', 'dc_id']:
        raise RuntimeError('balancer.get_domains_for_dc changed')

    def get_domains_for_dc(self, dc_id):
        return iter(health.order(list(orig(self, dc_id))))

    cls.get_domains_for_dc = get_domains_for_dc
    applied.append('cf-domain-health')


def _patch_ws_connect(raw_websocket, balancer):
    cls = raw_websocket.RawWebSocket
    orig = cls.connect
    params = list(inspect.signature(orig).parameters)
    if params[:2] != ['host', 'domain'] or not inspect.iscoroutinefunction(orig):
        raise RuntimeError('RawWebSocket.connect changed')

    async def connect(*args, **kwargs):
        host = args[0] if args else kwargs.get('host')
        domain = args[1] if len(args) > 1 else kwargs.get('domain')
        base = _cf_base(balancer, host, domain)
        try:
            ws = await orig(*args, **kwargs)
        except Exception:
            if base:
                health.failed(base)
            raise
        if base:
            health.ok(base)
        return ws

    cls.connect = staticmethod(connect)


def _patch_h2_preflight(cf_h2):
    lane_cls = cf_h2._HttpLane
    orig = lane_cls._preflight
    if list(inspect.signature(orig).parameters) != ['self']:
        raise RuntimeError('_HttpLane._preflight changed')

    async def _preflight(self):
        host = getattr(self, 'host', '')
        base = host.partition('.')[2] if isinstance(host, str) and host.startswith('kws') else ''
        started = time.monotonic()
        try:
            await orig(self)
        except asyncio.CancelledError:
            # CfH2Pool.open gives the whole setup 8 s: a dead domain hangs until then
            if base and time.monotonic() - started >= H2_HANG_SECONDS:
                health.failed(base)
            raise
        except (OSError, asyncio.TimeoutError):
            if base:
                health.failed(base)
            raise
        except Exception as exc:
            # httpx connect / timeout errors mean the domain is unreachable;
            # an HTTP status or protocol mismatch says nothing about WS
            if base and type(exc).__name__ in ('ConnectError', 'ConnectTimeout', 'ReadTimeout', 'PoolTimeout'):
                health.failed(base)
            raise
        if base:
            health.ok(base)

    lane_cls._preflight = _preflight
    applied.append('h2-domain-health')


async def _quiet_close(ws):
    try:
        await ws.close()
    except Exception:
        pass


def _patch_cf_fallback(bridge):
    orig = bridge._cfproxy_fallback
    expected = ['reader', 'writer', 'relay_init', 'label', 'ctx', 'dc', 'is_media', 'splitter']
    if list(inspect.signature(orig).parameters) != expected:
        raise RuntimeError('bridge._cfproxy_fallback changed')
    for name in ('balancer', 'RawWebSocket', 'proxy_config', 'stats', 'bridge_ws_reencrypt'):
        if not hasattr(bridge, name):
            raise RuntimeError('bridge.%s missing' % name)

    async def race(dc, label, media_tag, secure):
        domains = list(bridge.balancer.get_domains_for_dc(dc))
        loop = asyncio.get_running_loop()
        deadline = loop.time() + DEADLINE
        queue = iter(domains)
        attempts = {}
        state = {'more': True, 'tried': 0}
        winner = None
        target = 1

        def launch():
            base = next(queue, None)
            if base is None:
                state['more'] = False
                return
            host = 'kws%d.%s' % (dc, base)
            task = asyncio.ensure_future(bridge.RawWebSocket.connect(
                host, host, timeout=ATTEMPT_TIMEOUT, secure=secure))
            attempts[task] = (base, host)
            state['tried'] += 1

        def top_up():
            while state['more'] and len(attempts) < target:
                launch()

        top_up()
        try:
            while attempts:
                left = deadline - loop.time()
                if left <= 0:
                    break
                hedge = state['more'] and target < MAX_PARALLEL
                done, _ = await asyncio.wait(
                    list(attempts), timeout=min(STAGGER, left) if hedge else left,
                    return_when=asyncio.FIRST_COMPLETED)
                if not done:
                    if hedge:
                        target += 1
                        top_up()
                    continue
                for task in done:
                    base, host = attempts.pop(task)
                    if task.cancelled():
                        continue
                    exc = task.exception()
                    if exc is not None:
                        log.warning('[%s] DC%d%s CF proxy failed: %r (host=%s port=%d)',
                                    label, dc, media_tag, exc, host, 443 if secure else 80)
                    elif winner is None:
                        winner = (task.result(), base)
                    else:
                        await _quiet_close(task.result())
                if winner is not None:
                    break
                top_up()
        finally:
            losers = list(attempts)
            for task in losers:
                task.cancel()
            if losers:
                for result in await asyncio.gather(*losers, return_exceptions=True):
                    if isinstance(result, bridge.RawWebSocket):
                        await _quiet_close(result)
        if winner is None:
            log.warning('[%s] DC%d%s CF proxy: no domain answered (%d of %d tried in %.0fs)',
                        label, dc, media_tag, state['tried'], len(domains), DEADLINE)
        return winner

    async def _cfproxy_fallback(reader, writer, relay_init, label, ctx,
                                dc, is_media, splitter=None):
        media_tag = ' media' if is_media else ''
        secure = not bridge.proxy_config.disable_secure
        log.info('[%s] DC%d%s -> trying CF proxy', label, dc, media_tag)
        started = time.monotonic()
        found = await race(dc, label, media_tag, secure)
        if found is None:
            return False
        ws, base = found
        log.debug('[%s] WS CONNECT DC%d%s route=cf setup_ms=%.0f',
                  label, dc, media_tag, (time.monotonic() - started) * 1000)
        if bridge.balancer.update_domain_for_dc(dc, base):
            log.info('[%s] Switched active CF domain', label)
        bridge.stats.connections_cfproxy += 1
        try:
            await ws.send(relay_init)
        except BaseException:
            await _quiet_close(ws)
            raise
        await bridge.bridge_ws_reencrypt(reader, writer, ws, label, ctx,
                                         dc=dc, is_media=is_media, splitter=splitter)
        return True

    bridge._cfproxy_fallback = _cfproxy_fallback
    applied.append('cf-race')


def _patch_ws_pool(pool):
    cls = pool._WsPool
    if not isinstance(getattr(cls, 'REFILL_BACKOFF_MAX', None), (int, float)):
        raise RuntimeError('_WsPool.REFILL_BACKOFF_MAX missing')
    if cls.REFILL_BACKOFF_MAX > WS_POOL_BACKOFF_MAX:
        cls.REFILL_BACKOFF_MAX = WS_POOL_BACKOFF_MAX
        applied.append('ws-pool-backoff')


def _patch_startup_log(pool):
    cls = pool._WsPool
    orig = cls.warmup
    if not inspect.iscoroutinefunction(orig):
        raise RuntimeError('_WsPool.warmup changed')

    async def warmup(self):
        log.info('kidpi tweaks: %s%s', ', '.join(applied) or 'none',
                 ('; skipped: ' + '; '.join(skipped)) if skipped else '')
        return await orig(self)

    cls.warmup = warmup


def _apply():
    from proxy import balancer, bridge, cf_h2, pool, raw_websocket

    steps = [
        ('cf-domain-health', lambda: _patch_balancer(balancer)),
        ('cf-domain-health', lambda: _patch_ws_connect(raw_websocket, balancer.balancer)),
        ('h2-domain-health', lambda: _patch_h2_preflight(cf_h2)),
        ('cf-race', lambda: _patch_cf_fallback(bridge)),
        ('ws-pool-backoff', lambda: _patch_ws_pool(pool)),
        ('startup-log', lambda: _patch_startup_log(pool)),
    ]
    for name, step in steps:
        try:
            step()
        except Exception as exc:
            skipped.append('%s (%s)' % (name, exc))


try:
    _apply()
except Exception as _exc:  # the proxy must start even if kidpi's tweaks cannot
    skipped.append('all (%r)' % (_exc,))
