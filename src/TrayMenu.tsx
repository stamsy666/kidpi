import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, type AppState, type TrayAction } from "./api";
import { sounds } from "./sounds";
import { PowerIcon, type PowerIconHandle } from "./components/PowerIcon";
import { SwitchOffToSwitchTransitionIcon, SwitchToSwitchOffTransitionIcon } from "./components/SwitchIcon";
import logo from "./assets/logo.png";

/** kidpi's own tray menu, opened by a right click on the tray icon. Hover is
 *  pure CSS, so moving the mouse never re-renders it (no flicker). */
export default function TrayMenu() {
  const [st, setSt] = useState<AppState | null>(null);
  const [seq, setSeq] = useState(0);
  const power = useRef<PowerIconHandle>(null);

  useEffect(() => {
    api.getState().then(setSt);
    const subs = [
      listen<AppState>("state", (e) => setSt(e.payload)),
      listen("tray-shown", () => {
        api.getState().then(setSt);
        setSeq((n) => n + 1); // replays the opening animation
      }),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);

  useEffect(() => {
    if (st) sounds.enabled = st.sounds;
  }, [st?.sounds]);

  const on = st?.phase === "on";

  const act = (a: TrayAction) => {
    if (a === "toggle") {
      sounds.play(on ? "toggleOff" : "toggleOn");
      window.setTimeout(() => power.current?.startAnimation(), 30);
    } else {
      sounds.play("tap");
      getCurrentWindow().hide();
    }
    api.trayAction(a);
  };

  return (
    <div className="tm" key={seq} role="menu" aria-label="kidpi">
      <div className="tm-head">
        <svg width="38" height="38" viewBox="0 0 40 40" aria-hidden="true">
          <rect width="40" height="40" rx="11" fill="#140a22" />
          <circle cx="23.7" cy="11.5" r="6" fill="#CFA0FF" opacity="0.35" />
          <circle cx="17.5" cy="25" r="6.2" fill="none" stroke="#CFA0FF" strokeWidth="3.2" />
          <path d="M23.7 13v9" stroke="#CFA0FF" strokeWidth="1.6" strokeLinecap="round" />
          <path d="M23.7 21v10" stroke="#CFA0FF" strokeWidth="3.2" strokeLinecap="round" />
          <circle cx="23.7" cy="11.5" r="2.6" fill="#faf2ff" />
        </svg>
        <div>
          <img src={logo} alt="kidpi" className="tm-logo" draggable={false} />
          <div className="tm-status">{on ? "Обход включён" : "Обход выключен"}</div>
        </div>
      </div>
      <div className="tm-div" aria-hidden="true" />

      <button
        type="button"
        role="menuitemcheckbox"
        aria-checked={on}
        className={`tm-item tm-toggle ${on ? "on" : "off"}`}
        onClick={() => act("toggle")}
        onMouseEnter={() => power.current?.startAnimation()}
      >
        <PowerIcon ref={power} size={20} strokeWidth={1.8} className="tm-ico" />
        <span className="grow">Обход</span>
        <span className={`tm-switch ${on ? "on" : "off"}`} aria-hidden="true">
          {on ? <SwitchOffToSwitchTransitionIcon key="on" size={38} /> : <SwitchToSwitchOffTransitionIcon key="off" size={38} />}
        </span>
      </button>

      <button type="button" role="menuitem" className="tm-item" onClick={() => act("open")}>
        <svg className="tm-ico" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <rect x="3" y="4" width="18" height="16" rx="3" />
          <path d="M3 9h18" />
        </svg>
        Открыть kidpi
      </button>
      <button type="button" role="menuitem" className="tm-item" onClick={() => act("repick")}>
        <svg className="tm-ico" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.34-4.34" />
        </svg>
        Подобрать стратегию заново
      </button>
      <div className="tm-div" aria-hidden="true" />
      <button type="button" role="menuitem" className="tm-item" onClick={() => act("settings")}>
        <svg className="tm-ico" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
        Настройки
      </button>
      <button type="button" role="menuitem" className="tm-item" onClick={() => act("quit")}>
        <svg className="tm-ico" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
          <path d="m16 17 5-5-5-5" />
          <path d="M21 12H9" />
        </svg>
        Выход
      </button>
    </div>
  );
}
