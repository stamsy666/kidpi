import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, SCALE_STEPS, type AppState, type LogEntry, type SettingKey } from "./api";
import { sounds } from "./sounds";
import StrategySelect from "./StrategySelect";
import { Switch } from "./components/SwitchIcon";

type Upd = "idle" | "checking" | "done" | "updated" | "error";

export default function Settings({
  st,
  setSt,
  closing,
  onClose,
  onRepick,
}: {
  st: AppState;
  setSt: (s: AppState) => void;
  closing: boolean;
  onClose: () => void;
  onRepick: () => void;
}) {
  const [log, setLog] = useState<LogEntry[]>([]);
  const [upd, setUpd] = useState<Upd>("idle");
  const [site, setSite] = useState<"ok" | "err" | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const timers = useRef<number[]>([]);

  useEffect(() => {
    api.getLog().then(setLog).catch(() => {});
    const un = listen<LogEntry>("log", (e) => setLog((l) => [e.payload, ...l].slice(0, 200)));
    return () => {
      un.then((f) => f());
      timers.current.forEach((t) => window.clearTimeout(t));
    };
  }, []);

  const later = (fn: () => void, ms: number) => {
    timers.current.push(window.setTimeout(fn, ms));
  };

  const toggle = (key: SettingKey, value: boolean) => {
    if (key === "sounds") {
      sounds.enabled = true;
      sounds.play(value ? "toggleOn" : "toggleOff");
      sounds.enabled = value;
    } else {
      sounds.play(value ? "toggleOn" : "toggleOff");
    }
    setSt({ ...st, [key]: value });
    api
      .setSetting(key, value)
      .then(setSt)
      .catch(() => {
        sounds.play("caution");
        api.getState().then(setSt);
      });
  };

  const siteResult = (ok: boolean, code?: string) => {
    setSite(ok ? "ok" : "err");
    if (!ok) {
      input.current?.animate(
        [
          { transform: "translateX(0)" },
          { transform: "translateX(-6px)" },
          { transform: "translateX(6px)" },
          { transform: "translateX(-4px)" },
          { transform: "translateX(4px)" },
          { transform: "translateX(0)" },
        ],
        { duration: 380, easing: "ease-out" },
      );
      sounds.play(code === "invalid" || (code ?? "").startsWith("io") ? "caution" : "disabled");
    }
    later(() => setSite(null), ok ? 900 : 1400);
  };

  const siteAction = (add: boolean) => {
    const v = input.current?.value ?? "";
    (add ? api.addSite(v) : api.removeSite(v))
      .then(() => {
        sounds.play(add ? "select" : "toggleOff");
        if (input.current) input.current.value = "";
        siteResult(true);
      })
      .catch((code: string) => siteResult(false, String(code)));
  };

  const checkUpdates = () => {
    if (upd === "checking") return;
    sounds.play("button");
    setUpd("checking");
    api
      .checkUpdates()
      .then((r) => {
        sounds.play("notify");
        setUpd(r.startsWith("updated") ? "updated" : "done");
        api.getState().then(setSt);
      })
      .catch(() => {
        sounds.play("caution");
        setUpd("error");
      })
      .finally(() => later(() => setUpd("idle"), 2400));
  };

  const [scaleMax, setScaleMax] = useState(2);
  useEffect(() => {
    api.scaleMax().then(setScaleMax).catch(() => {});
  }, []);
  const scaleIdx = Math.max(
    0,
    SCALE_STEPS.findIndex((v) => Math.abs(v - st.scale) < 0.001),
  );
  const scaleOk = (v: number) => v <= scaleMax + 0.001 || Math.abs(v - st.scale) < 0.001;
  const pickScale = (v: number) => {
    if (Math.abs(v - st.scale) < 0.001 || !scaleOk(v)) return;
    sounds.play("select");
    setSt({ ...st, scale: v });
    api
      .setScale(v)
      .then((applied) => {
        if (Math.abs(applied - v) > 0.001) api.getState().then(setSt);
      })
      .catch(() => {
        sounds.play("caution");
        api.getState().then(setSt);
      });
  };
  const scaleKey = (e: React.KeyboardEvent) => {
    const d = e.key === "ArrowRight" || e.key === "ArrowUp" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowDown" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const next = SCALE_STEPS[scaleIdx + d];
    if (next !== undefined && scaleOk(next)) {
      pickScale(next);
      const opts = (e.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>(".seg-opt");
      opts[scaleIdx + d]?.focus();
    }
  };

  const [tgNote, setTgNote] = useState<"unknown" | "open" | null>(null);
  const connectTg = () => {
    sounds.play("button");
    setTgNote(null);
    api
      .openTgLink()
      .then((how) => {
        if (how === "unknown") setTgNote("unknown");
      })
      .catch(() => {
        sounds.play("caution");
        setTgNote("open");
      });
  };

  const [copied, setCopied] = useState(false);
  const [installing, setInstalling] = useState(false);
  const installApp = () => {
    if (installing) return;
    sounds.play("button");
    setInstalling(true);
    // on success the installer takes over and kidpi restarts
    api.installAppUpdate().catch(() => {
      sounds.play("caution");
      setInstalling(false);
    });
  };
  const copyTgLink = () => {
    if (!st.tgLink) return;
    sounds.play("tap");
    navigator.clipboard
      ?.writeText(st.tgLink)
      .then(() => {
        setCopied(true);
        later(() => setCopied(false), 1600);
      })
      .catch(() => sounds.play("caution"));
  };

  const copyLog = () => {
    sounds.play("tap");
    const text = [...log].reverse().map((e) => `${e.time} ${e.text}`).join("\n");
    navigator.clipboard?.writeText(text).catch(() => {});
  };

  return (
    <div className={`overlay ${closing ? "closing" : ""}`}>
      <button type="button" className="backdrop-btn" aria-label="Закрыть настройки" onClick={onClose} />
      <aside className="panel" aria-label="Настройки">
        <div className="panel-scroll scroll">
          <div className="panel-head">
            <h2>Настройки</h2>
            <button type="button" className="icon-btn" aria-label="Закрыть настройки" onClick={onClose}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                <path d="M6 6l12 12" />
                <path d="M18 6 6 18" />
              </svg>
            </button>
          </div>

          <section className="section">
            <h3>Стратегия</h3>
            <div className="card">
              <span className="label">Текущая стратегия</span>
              <StrategySelect
                value={st.strategy}
                options={st.strategies}
                onChange={(name) => {
                  setSt({ ...st, strategy: name });
                  api.setStrategy(name).then(setSt).catch(() => api.getState().then(setSt));
                }}
              />
              <button type="button" className="btn outline" onClick={onRepick}>
                Подобрать заново
              </button>
            </div>
          </section>

          <section className="section">
            <h3>Сайты</h3>
            <div className="card">
              <input
                ref={input}
                type="text"
                className={`input ${site ?? ""}`}
                placeholder="Вставьте ссылку"
                aria-label="Ссылка на сайт"
                spellCheck={false}
                autoComplete="off"
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    siteAction(true);
                  }
                }}
              />
              <div className="btn-row">
                <button type="button" className="btn primary" onClick={() => siteAction(true)}>
                  Добавить
                </button>
                <button type="button" className="btn" onClick={() => siteAction(false)}>
                  Удалить
                </button>
              </div>
            </div>
          </section>

          <section className="section">
            <h3>Telegram</h3>
            <div className="card">
              <div className="toggle-row flush">
                <span>Ускорение Telegram</span>
                <Switch on={st.telegram} label="Ускорение Telegram" onToggle={() => toggle("telegram", !st.telegram)} />
              </div>
              {st.telegram && (
                <div className="btn-col reveal">
                  <button type="button" className="btn primary" onClick={connectTg}>
                    Подключить
                  </button>
                  <button type="button" className="btn" onClick={copyTgLink}>
                    {copied ? "Скопировано" : "Скопировать ссылку"}
                  </button>
                </div>
              )}
              {st.telegram && tgNote && (
                <p className={`note small reveal ${tgNote === "open" ? "err" : ""}`}>
                  {tgNote === "open"
                    ? "Не получилось открыть Telegram. Скопируйте ссылку, отправьте её себе в «Избранное» и нажмите на неё."
                    : "Telegram Desktop не найден. Если он не открылся, скопируйте ссылку, отправьте её себе в «Избранное» и нажмите на неё."}
                </p>
              )}
              {st.telegram && st.tgError && <p className="err small">{st.tgError}</p>}
            </div>
          </section>

          <section className="section">
            <h3>Обновления</h3>
            <div className="card">
              <div className="kv">
                <span>zapret</span>
                <span>{st.zapretVersion}</span>
              </div>
              <div className="kv">
                <span>tg-ws-proxy</span>
                <span>{st.tgVersion}</span>
              </div>
              <div className="kv">
                <span>kidpi</span>
                <span>{st.appVersion}</span>
              </div>
              <div className="toggle-row divided">
                <span>Обновлять автоматически</span>
                <Switch on={st.autoUpdate} label="Обновлять автоматически" onToggle={() => toggle("autoUpdate", !st.autoUpdate)} />
              </div>
              {st.appUpdate && (
                <button type="button" className="btn primary reveal" onClick={installApp} disabled={installing}>
                  {installing ? (
                    <>
                      <svg className="spin-fast" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" aria-hidden="true">
                        <path d="M21 12a9 9 0 1 1-6.2-8.56" />
                      </svg>
                      <span>Устанавливаю…</span>
                    </>
                  ) : (
                    <span>Установить kidpi {st.appUpdate}</span>
                  )}
                </button>
              )}
              <button type="button" className="btn" onClick={checkUpdates} aria-live="polite">
                {upd === "idle" && <span>Проверить обновления</span>}
                {upd === "checking" && (
                  <>
                    <svg className="spin-fast" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#CFA0FF" strokeWidth="2.6" strokeLinecap="round" aria-hidden="true">
                      <path d="M21 12a9 9 0 1 1-6.2-8.56" />
                    </svg>
                    <span>Проверяю…</span>
                  </>
                )}
                {(upd === "done" || upd === "updated") && (
                  <>
                    <svg className="pop" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#CFA0FF" strokeWidth="2.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                      <path d="M20 6 9 17l-5-5" />
                    </svg>
                    <span>{upd === "updated" ? "Обновлено" : "Всё актуально"}</span>
                  </>
                )}
                {upd === "error" && <span>Нет связи с GitHub</span>}
              </button>
            </div>
          </section>

          <section className="section">
            <h3>Запуск и трей</h3>
            <div className="card rows">
              <div className="toggle-row">
                <span>Запускать вместе с Windows</span>
                <Switch on={st.autostart} label="Запускать вместе с Windows" onToggle={() => toggle("autostart", !st.autostart)} />
              </div>
              <div className="toggle-row">
                <span>Сворачивать в трей при закрытии</span>
                <Switch on={st.closeToTray} label="Сворачивать в трей при закрытии" onToggle={() => toggle("closeToTray", !st.closeToTray)} />
              </div>
            </div>
          </section>

          <section className="section">
            <h3>Масштаб</h3>
            <div className="card">
              <div
                className="seg"
                role="radiogroup"
                aria-label="Масштаб интерфейса"
                onKeyDown={scaleKey}
                style={{ "--n": SCALE_STEPS.length, "--i": scaleIdx } as React.CSSProperties}
              >
                <span className="seg-thumb" aria-hidden="true" />
                {SCALE_STEPS.map((v, i) => (
                  <button
                    key={v}
                    type="button"
                    role="radio"
                    aria-checked={i === scaleIdx}
                    tabIndex={i === scaleIdx ? 0 : -1}
                    className="seg-opt"
                    disabled={!scaleOk(v)}
                    title={scaleOk(v) ? undefined : "Не помещается на этом экране"}
                    onClick={() => pickScale(v)}
                  >
                    {Math.round(v * 100)}%
                  </button>
                ))}
              </div>
            </div>
          </section>

          <section className="section">
            <h3>Дополнительно</h3>
            <div className="card rows">
              <div className="toggle-row">
                <span>Игровой режим</span>
                <Switch on={st.gameMode} label="Игровой режим" onToggle={() => toggle("gameMode", !st.gameMode)} />
              </div>
              <div className="toggle-row">
                <span>Интро при запуске</span>
                <Switch on={st.intro} label="Интро при запуске" onToggle={() => toggle("intro", !st.intro)} />
              </div>
              <div className="toggle-row">
                <span>Звуки</span>
                <Switch on={st.sounds} label="Звуки" onToggle={() => toggle("sounds", !st.sounds)} />
              </div>
            </div>
          </section>

          <section className="section">
            <h3>Журнал</h3>
            <div className="card log-card">
              <ul className="log">
                {log.slice(0, 8).map((e, i) => (
                  <li key={`${i}-${e.time}-${e.text}`}>
                    <span className="t">{e.time}</span>
                    <span>{e.text}</span>
                  </li>
                ))}
                {log.length === 0 && <li className="empty">Пока пусто</li>}
              </ul>
              <div className="btn-row">
                <button type="button" className="btn" onClick={copyLog}>
                  Скопировать
                </button>
                <button
                  type="button"
                  className="btn"
                  onClick={() => {
                    sounds.play("tap");
                    api.openLogFolder();
                  }}
                >
                  Папка с логами
                </button>
              </div>
            </div>
          </section>
        </div>
      </aside>
    </div>
  );
}
