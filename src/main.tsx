import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/nunito/500.css";
import "@fontsource/nunito/600.css";
import "@fontsource/nunito/700.css";
import "@fontsource/nunito/800.css";
import "./styles.css";
import { api } from "./api";
import { sounds } from "./sounds";
import App from "./App";
import TrayMenu from "./TrayMenu";

// The tray menu is a second, transparent window that loads index.html#tray.
const isTray = window.location.hash === "#tray";
document.body.classList.add(isTray ? "tray" : "main");

// No browser context menu or reload shortcuts inside an app window.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement | null;
  if (!t || t.tagName !== "INPUT") e.preventDefault();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "F5" || (e.ctrlKey && (e.key === "r" || e.key === "R"))) e.preventDefault();
});

// The interface scale is applied and the intro decided before the first paint, so
// the window never flashes at 100 % or shows the app before its intro.
const timeout = <T,>(p: Promise<T>, fallback: T) =>
  Promise.race([p.catch(() => fallback), new Promise<T>((r) => setTimeout(() => r(fallback), 400))]);
// The intro's sounds follow the "Звуки" setting from its first note.
const soundSetting = isTray
  ? Promise.resolve()
  : timeout(
      api.getState().then((s) => {
        sounds.enabled = s.sounds;
      }),
      undefined,
    );
Promise.all([timeout(api.syncZoom(), undefined), isTray ? false : timeout(api.takeIntro(), false), soundSetting]).then(([, intro]) => {
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>{isTray ? <TrayMenu /> : <App intro={intro} />}</React.StrictMode>,
  );
});
