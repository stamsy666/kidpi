//! Checks whether Discord and YouTube are reachable right now, the same
//! endpoints zapret's own `utils/targets.txt` test uses.

use std::time::{Duration, Instant};

/// (service, url)
const TARGETS: &[(&str, &str)] = &[
    ("Discord", "https://discord.com"),
    ("Discord", "https://gateway.discord.gg"),
    ("Discord", "https://cdn.discordapp.com"),
    ("YouTube", "https://www.youtube.com"),
    ("YouTube", "https://i.ytimg.com"),
    ("YouTube", "https://redirector.googlevideo.com"),
];

/// A site that normally works without any bypass: if it fails too, the
/// internet itself is down and there is nothing to fix.
const CONTROL: &str = "https://ya.ru";

#[derive(Clone, Debug, Default)]
pub struct Score {
    pub ok: u32,
    pub total: u32,
    pub ms: u128,
    pub discord_ok: bool,
    pub youtube_ok: bool,
}

fn client(timeout: Duration) -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout)
        // a fresh connection per check: a pooled one would hide a new block
        .pool_max_idle_per_host(0)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
        .build()
        .ok()
}

async fn probe(c: &reqwest::Client, url: &str) -> Option<u128> {
    let t = Instant::now();
    // Any HTTP answer means the TLS handshake went through the DPI.
    c.get(url).send().await.ok().map(|_| t.elapsed().as_millis())
}

pub async fn check_all(timeout_ms: u64) -> Score {
    let Some(c) = client(Duration::from_millis(timeout_ms)) else {
        return Score { total: TARGETS.len() as u32, ..Default::default() };
    };
    let results = futures::future::join_all(TARGETS.iter().map(|(_, u)| probe(&c, u))).await;
    let mut s = Score { total: TARGETS.len() as u32, discord_ok: true, youtube_ok: true, ..Default::default() };
    for ((svc, _), r) in TARGETS.iter().zip(results) {
        match r {
            Some(ms) => {
                s.ok += 1;
                s.ms += ms;
            }
            None => {
                if *svc == "Discord" {
                    s.discord_ok = false;
                } else {
                    s.youtube_ok = false;
                }
                s.ms += timeout_ms as u128;
            }
        }
    }
    s
}

pub async fn internet_ok() -> bool {
    match client(Duration::from_secs(5)) {
        Some(c) => probe(&c, CONTROL).await.is_some(),
        None => false,
    }
}

impl Score {
    /// More reachable endpoints wins; on a tie, the faster one.
    pub fn better_than(&self, other: &Score) -> bool {
        self.ok > other.ok || (self.ok == other.ok && self.ms < other.ms)
    }
}
