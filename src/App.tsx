import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import NightSceneBacklit from "./components/NightSceneBacklit";
import { SearchIcon, type SearchIconHandle } from "./components/SearchIcon";
import { PowerIcon, type PowerIconHandle } from "./components/PowerIcon";
import Settings from "./Settings";
import Intro from "./Intro";
import { api, type AppState } from "./api";
import { sounds } from "./sounds";
import logo from "./assets/logo.png";

type Win = "normal" | "collapsing" | "hidden" | "restoring";
const RING = 653.45; // 2·π·104

export default function App({ intro = false }: { intro?: boolean }) {
  const [st, setSt] = useState<AppState | null>(null);
  // launch intro: "play" — only the intro; "fly" — the app appears under it; "done"
  const [introStage, setIntroStage] = useState<"play" | "fly" | "done">(
    intro ? "play" : "done",
  );
  const shown = introStage !== "play";
  const [settings, setSettings] = useState<"closed" | "open" | "closing">(
    "closed",
  );
  const [toast, setToast] = useState<"none" | "open" | "closing">("none");
  const [win, setWin] = useState<Win>("normal");
  const [maxed, setMaxed] = useState(false);
  const stRef = useRef(st);
  const winRef = useRef(win);
  stRef.current = st;
  winRef.current = win;

  const search = useRef<SearchIconHandle>(null);
  const power = useRef<PowerIconHandle>(null);
  const prevPhase = useRef<string | null>(null);

  // ---- window: collapse into the tray / minimize, and come back ----
  const collapse = (mode: "hide" | "minimize") => {
    if (winRef.current === "collapsing" || winRef.current === "hidden") return;
    sounds.play("down");
    setSettings("closed");
    setWin("collapsing");
    window.setTimeout(() => {
      setWin("hidden");
      if (mode === "hide") api.hideMain();
      else api.minimizeMain();
    }, 340);
  };

  const restore = () => {
    if (winRef.current !== "hidden" && winRef.current !== "collapsing") return;
    sounds.play("up");
    setWin("restoring");
    window.setTimeout(
      () => setWin((w) => (w === "restoring" ? "normal" : w)),
      520,
    );
  };

  const requestClose = () => {
    const s = stRef.current;
    if (!s) return;
    if (!s.closeToTray) {
      api.quitApp();
      return;
    }
    if (!s.trayNotified) {
      // explained once, then the window just goes to the tray
      sounds.play("notify");
      setSettings("closed");
      setToast("open");
      setSt({ ...s, trayNotified: true });
      api.markTrayNotified();
      return;
    }
    collapse("hide");
  };

  const closeToast = () => {
    sounds.play("button");
    setToast("closing");
    window.setTimeout(() => {
      setToast("none");
      collapse("hide");
    }, 260);
  };

  // ---- settings panel ----
  const openSettings = () => {
    if (settings !== "closed") return;
    sounds.play("up");
    setSettings("open");
  };
  const closeSettings = (after?: () => void) => {
    if (settings !== "open") return;
    sounds.play("down");
    setSettings("closing");
    window.setTimeout(() => {
      setSettings("closed");
      after?.();
    }, 280);
  };

  const handlers = useRef({ requestClose, restore, openSettings });
  handlers.current = { requestClose, restore, openSettings };

  useEffect(() => {
    sounds.preload();
    api.getState().then(setSt);
    const subs = [
      listen<AppState>("state", (e) => setSt(e.payload)),
      listen("close-requested", () => handlers.current.requestClose()),
      listen("window-shown", () => handlers.current.restore()),
      listen("open-settings", () => handlers.current.openSettings()),
      getCurrentWindow().onFocusChanged(({ payload }) => {
        if (payload) handlers.current.restore();
      }),
      getCurrentWindow().onResized(() => {
        getCurrentWindow()
          .isMaximized()
          .then(setMaxed)
          .catch(() => {});
      }),
    ];
    getCurrentWindow()
      .isMaximized()
      .then(setMaxed)
      .catch(() => {});
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);

  useEffect(() => {
    if (st) sounds.enabled = st.sounds;
  }, [st?.sounds]);

  // ---- sounds and icon nudges when the phase changes ----
  const phase = st?.phase ?? null;
  useEffect(() => {
    const prev = prevPhase.current;
    prevPhase.current = phase;
    if (!phase) return;
    if (phase === "picking" && prev !== "picking") sounds.loopStart();
    if (phase !== "picking" && prev === "picking") {
      sounds.loopStop();
      if (phase === "on") sounds.play("success");
      else if (stRef.current?.pickFailed) sounds.play("caution");
    }
    if (phase === "pick") {
      const t = window.setTimeout(() => search.current?.startAnimation(), 1100);
      return () => window.clearTimeout(t);
    }
    if (phase === "on" || phase === "off") {
      const t = window.setTimeout(() => power.current?.startAnimation(), 450);
      return () => window.clearTimeout(t);
    }
  }, [phase]);

  const startPick = () => {
    sounds.play("button");
    api.startPick();
  };
  const cancelPick = () => {
    sounds.loopStop();
    sounds.play("button");
    api.cancelPick();
  };
  const toggle = (on: boolean) => {
    sounds.play(on ? "toggleOn" : "toggleOff");
    api.setEnabled(on).then(setSt);
  };

  const prog = st?.progress;
  const total = prog?.total || st?.strategies.length || 0;
  const index = prog?.index ?? 0;

  return (
    <div className="app">
      <div className={`win ${win}`}>
        <div className="bg" aria-hidden="true">
          {win !== "hidden" && (
            <NightSceneBacklit style={{ minWidth: 0, minHeight: 0 }} />
          )}
        </div>
        <div className="scrim" aria-hidden="true" />

        {shown && (
          <>
            <header className="titlebar" data-tauri-drag-region>
              <img
                className={`logo ${introStage === "fly" ? "landing" : ""} ${intro ? "no-enter" : ""}`}
                src={logo}
                alt="kidpi"
                data-tauri-drag-region
                draggable={false}
              />
              <div className="tb-actions">
                <button
                  type="button"
                  className={`icon-btn gear ${settings === "open" ? "turned" : ""} ${st?.appUpdate ? "has-dot" : ""}`}
                  aria-label={
                    st?.appUpdate
                      ? "Настройки — доступно обновление kidpi"
                      : "Настройки"
                  }
                  onClick={() =>
                    settings === "open" ? closeSettings() : openSettings()
                  }
                >
                  <svg
                    width="20"
                    height="20"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.8"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-hidden="true"
                  >
                    <circle cx="12" cy="12" r="3" />
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                  </svg>
                </button>
                <span className="sep" aria-hidden="true" />
                <button
                  type="button"
                  className="icon-btn"
                  aria-label="Свернуть окно"
                  onClick={() => collapse("minimize")}
                >
                  <svg
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    aria-hidden="true"
                  >
                    <path d="M5 12h14" />
                  </svg>
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={
                    maxed
                      ? "Вернуть обычный размер"
                      : "Развернуть на весь экран"
                  }
                  onClick={() => {
                    sounds.play("tap");
                    getCurrentWindow().toggleMaximize();
                  }}
                >
                  {maxed ? (
                    <svg
                      width="16"
                      height="16"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      aria-hidden="true"
                    >
                      <rect x="4" y="8" width="12" height="12" rx="2" />
                      <path d="M8 8V6a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2" />
                    </svg>
                  ) : (
                    <svg
                      width="16"
                      height="16"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      aria-hidden="true"
                    >
                      <rect x="5" y="5" width="14" height="14" rx="2" />
                    </svg>
                  )}
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label="Закрыть окно"
                  onClick={requestClose}
                >
                  <svg
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    aria-hidden="true"
                  >
                    <path d="M6 6l12 12" />
                    <path d="M18 6 6 18" />
                  </svg>
                </button>
              </div>
            </header>

            <main className="center">
              {st && st.phase === "pick" && (
                <div className="phase" key="pick">
                  <div className="orb-wrap breathe">
                    <svg
                      className="ring-dashed"
                      width="256"
                      height="256"
                      viewBox="0 0 256 256"
                      aria-hidden="true"
                    >
                      <circle
                        cx="128"
                        cy="128"
                        r="124"
                        fill="none"
                        stroke="rgba(207,160,255,0.5)"
                        strokeWidth="1.6"
                        strokeLinecap="round"
                        strokeDasharray="1.5 11"
                      />
                    </svg>
                    <span className="ripple r1" aria-hidden="true" />
                    <button
                      type="button"
                      className="orb pick"
                      onClick={startPick}
                      onMouseEnter={() => search.current?.startAnimation()}
                    >
                      <SearchIcon
                        ref={search}
                        size={34}
                        color="#CFA0FF"
                        style={{ display: "flex", pointerEvents: "none" }}
                      />
                      Подобрать
                    </button>
                  </div>
                  <div className="text">
                    <h1 className="title">Найдём, что работает у вас</h1>
                    {st.pickFailed ? (
                      <p className="sub">
                        Ни одна стратегия не открыла Discord и YouTube.
                        Проверьте интернет и попробуйте ещё раз.
                      </p>
                    ) : (
                      <p className="sub">
                        kidpi по очереди проверит все стратегии обхода и включит
                        именно ту, с которой у вас будет большая совместимость.
                        До минуты.
                      </p>
                    )}
                    {st.error && <p className="err">{st.error}</p>}
                  </div>
                </div>
              )}

              {st && st.phase === "picking" && (
                <div className="phase" key="picking">
                  <div
                    className="progress"
                    role="progressbar"
                    aria-label="Подбор стратегии"
                    aria-valuemin={0}
                    aria-valuemax={total}
                    aria-valuenow={index}
                  >
                    <svg
                      width="220"
                      height="220"
                      viewBox="0 0 220 220"
                      aria-hidden="true"
                      style={{
                        position: "absolute",
                        inset: 0,
                        transform: "rotate(-90deg)",
                      }}
                    >
                      <circle
                        cx="110"
                        cy="110"
                        r="104"
                        fill="none"
                        stroke="rgba(255,255,255,0.12)"
                        strokeWidth="4"
                      />
                      <circle
                        className="ring-progress"
                        cx="110"
                        cy="110"
                        r="104"
                        fill="none"
                        stroke="#CFA0FF"
                        strokeWidth="4"
                        strokeLinecap="round"
                        strokeDasharray={RING}
                        strokeDashoffset={
                          total ? RING * (1 - index / total) : RING
                        }
                      />
                    </svg>
                    <svg
                      className="spinner"
                      width="220"
                      height="220"
                      viewBox="0 0 220 220"
                      aria-hidden="true"
                    >
                      <circle
                        cx="110"
                        cy="110"
                        r="90"
                        fill="none"
                        stroke="rgba(207,160,255,0.4)"
                        strokeWidth="2"
                        strokeLinecap="round"
                        strokeDasharray="70 495.5"
                      />
                    </svg>
                    <div className="progress-num">
                      <span className="num" key={index}>
                        {index}
                      </span>
                      <span className="of">из {total}</span>
                    </div>
                  </div>
                  <div className="text">
                    <h1 className="title">Подбираю стратегию…</h1>
                    <p className="sub" key={prog?.name}>
                      Проверяю {prog?.name || "…"}: Discord и YouTube
                    </p>
                  </div>
                  <button
                    type="button"
                    className="ghost-btn"
                    onClick={cancelPick}
                  >
                    Отменить
                  </button>
                </div>
              )}

              {st && (st.phase === "on" || st.phase === "off") && (
                <div className="phase" key={st.phase}>
                  <div
                    className={`orb-wrap ${st.phase === "on" ? "breathe" : ""}`}
                  >
                    {st.phase === "on" && (
                      <>
                        <span className="burst" aria-hidden="true" />
                        <span className="ripple r1" aria-hidden="true" />
                        <span className="ripple r2" aria-hidden="true" />
                      </>
                    )}
                    <button
                      type="button"
                      className={`orb ${st.phase}`}
                      aria-pressed={st.phase === "on"}
                      aria-label={
                        st.phase === "on" ? "Выключить обход" : "Включить обход"
                      }
                      onClick={() => toggle(st.phase !== "on")}
                      onMouseEnter={() => power.current?.startAnimation()}
                    >
                      <PowerIcon
                        ref={power}
                        size={70}
                        strokeWidth={1.5}
                        style={{ pointerEvents: "none" }}
                      />
                    </button>
                  </div>
                  <div className="text">
                    <h1 className="title">
                      {st.phase === "on" ? "Обход включён" : "Обход выключен"}
                    </h1>
                    <p className="sub">
                      {st.phase === "on"
                        ? `Стратегия ${st.strategy}`
                        : `Нажмите на кнопку, чтобы включить · стратегия ${st.strategy}`}
                    </p>
                    {st.error && <p className="err">{st.error}</p>}
                  </div>
                </div>
              )}
            </main>

            <footer className="credit">
              developed by{" "}
              <a
                className="credit-link"
                href="https://t.me/kidencedev"
                onClick={(e) => {
                  e.preventDefault();
                  sounds.play("tap");
                  api.openUrl("https://t.me/kidencedev");
                }}
              >
                @kidencedev
                <span className="credit-tip" aria-hidden="true">
                  t.me/kidencedev
                </span>
              </a>
            </footer>
          </>
        )}

        {st && settings !== "closed" && (
          <Settings
            st={st}
            setSt={setSt}
            closing={settings === "closing"}
            onClose={() => closeSettings()}
            onRepick={() => closeSettings(() => api.startPick())}
          />
        )}

        {introStage !== "done" && (
          <Intro
            onFly={() => setIntroStage("fly")}
            onDone={() => setIntroStage("done")}
          />
        )}

        {toast !== "none" && (
          <div className={`toast-bg ${toast === "closing" ? "closing" : ""}`}>
            <div className="toast">
              <svg
                width="56"
                height="56"
                viewBox="0 0 40 40"
                aria-hidden="true"
              >
                <rect width="40" height="40" rx="11" fill="#140a22" />
                <circle
                  cx="23.7"
                  cy="11.5"
                  r="6"
                  fill="#CFA0FF"
                  opacity="0.35"
                />
                <circle
                  cx="17.5"
                  cy="25"
                  r="6.2"
                  fill="none"
                  stroke="#CFA0FF"
                  strokeWidth="3.2"
                />
                <path
                  d="M23.7 13v9"
                  stroke="#CFA0FF"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                />
                <path
                  d="M23.7 21v10"
                  stroke="#CFA0FF"
                  strokeWidth="3.2"
                  strokeLinecap="round"
                />
                <circle cx="23.7" cy="11.5" r="2.6" fill="#faf2ff" />
              </svg>
              <h2>kidpi работает в трее</h2>
              <p>Окно закрыто, но обход продолжает работать.</p>
              <button
                type="button"
                className="btn primary pill"
                onClick={closeToast}
              >
                Закрыть окно
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
