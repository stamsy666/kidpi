import { useEffect, useMemo, useRef } from "react";
import logo from "./assets/logo.png";
import { sounds } from "./sounds";

/**
 * Launch intro (~3 s): a point of lavender light wakes in the dark, petals drift
 * in toward it, the light blooms and throws them outward, the kidpi logo comes
 * out of the glow and flies into its place in the title bar while the app
 * appears underneath. A click or any key skips to the end.
 *
 * onFly: the app should mount its title bar now (the logo flies to `.titlebar .logo`).
 * onDone: the intro is over and can be removed.
 */

const BLOOM = 1450; // ms: the light blooms, petals are thrown outward
const LOGO = 1600; // ms: the logo comes out of the glow
const FLY = 2950; // ms: the logo leaves for the title bar
const FLY_MS = 720;

interface Petal {
  angle: number;
  near: number; // 0..1, how far into the scene the petal comes
  size: number;
  spin: number;
  delay: number;
  blur: boolean;
  hue: number;
}

function makePetals(n: number): Petal[] {
  return Array.from({ length: n }, (_, i) => ({
    angle: (i / n) * Math.PI * 2 + (Math.random() - 0.5) * 0.5,
    near: 0.2 + Math.random() * 0.45,
    size: 9 + Math.random() * 16,
    spin: (Math.random() < 0.5 ? -1 : 1) * (160 + Math.random() * 220),
    delay: Math.random() * 260,
    blur: Math.random() < 0.3,
    hue: Math.random(),
  }));
}

export default function Intro({ onFly, onDone }: { onFly: () => void; onDone: () => void }) {
  const root = useRef<HTMLDivElement>(null);
  const logoRef = useRef<HTMLImageElement>(null);
  const petals = useMemo(() => makePetals(26), []);
  const cb = useRef({ onFly, onDone });
  cb.current = { onFly, onDone };

  useEffect(() => {
    const el = root.current;
    const img = logoRef.current;
    if (!el || !img) return;
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const anims: Animation[] = [];
    const timers: number[] = [];
    const at = (ms: number, fn: () => void) => timers.push(window.setTimeout(fn, ms));
    const run = (node: Element | null, frames: Keyframe[], opts: KeyframeAnimationOptions) => {
      if (!node) return null;
      const a = node.animate(frames, { fill: "both", ...opts });
      anims.push(a);
      return a;
    };
    let flying = false;

    const fly = () => {
      if (flying) return;
      flying = true;
      timers.forEach((t) => window.clearTimeout(t));
      cb.current.onFly();
      // the logo is visible from here on, wherever its own animation was
      img.getAnimations().forEach((a) => a.finish());
      // let the title bar mount, then aim at its logo
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          const target = document.querySelector(".titlebar .logo");
          const from = img.getBoundingClientRect();
          let transform = "translate(0px, -40px) scale(0.5)";
          if (target && from.height > 0) {
            const to = target.getBoundingClientRect();
            const dx = to.left + to.width / 2 - (from.left + from.width / 2);
            const dy = to.top + to.height / 2 - (from.top + from.height / 2);
            transform = `translate(${dx}px, ${dy}px) scale(${to.height / from.height})`;
          }
          run(img, [{ transform: "translate(0, 0) scale(1)", filter: "drop-shadow(0 0 18px rgba(207,160,255,0.75))" }, { transform, filter: "drop-shadow(0 0 0 rgba(207,160,255,0))" }], {
            duration: reduced ? 1 : FLY_MS,
            easing: "cubic-bezier(0.65, 0, 0.35, 1)",
          });
          const fade = run(el.querySelector(".intro-veil"), [{ opacity: 1 }, { opacity: 0 }], { duration: reduced ? 200 : FLY_MS, easing: "ease-in-out" });
          run(el.querySelector(".intro-stage"), [{ opacity: 1 }, { opacity: 0 }], { duration: reduced ? 200 : FLY_MS * 0.8, easing: "ease-out" });
          const end = () => cb.current.onDone();
          if (fade) fade.finished.then(end, end);
          else end();
        }),
      );
    };

    if (reduced) {
      run(img, [{ opacity: 0 }, { opacity: 1 }], { duration: 500, easing: "ease-out" });
      at(1300, fly);
    } else {
      const w = window.innerWidth;
      const h = window.innerHeight;
      const far = Math.hypot(w, h) * 0.62;
      const mid = Math.min(w, h) * 0.5;

      // the core of light, then the orb that blooms
      run(el.querySelector(".intro-core"), [
        { opacity: 0, transform: "scale(0.2)" },
        { opacity: 1, transform: "scale(1)", offset: 0.25 },
        { opacity: 1, transform: "scale(1.3)", offset: 0.85 },
        { opacity: 0, transform: "scale(2.6)" },
      ], { duration: BLOOM + 300, easing: "ease-out" });
      run(el.querySelector(".intro-orb"), [
        { opacity: 0, transform: "scale(0.06)" },
        { opacity: 0.9, transform: "scale(0.22)", offset: 0.13, easing: "ease-in-out" },
        { opacity: 1, transform: "scale(0.5)", offset: BLOOM / 2900, easing: "cubic-bezier(0.2, 0.9, 0.3, 1)" },
        { opacity: 1, transform: "scale(1.65)", offset: (BLOOM + 260) / 2900, easing: "ease-in-out" },
        { opacity: 0.3, transform: "scale(1.15)" },
      ], { duration: 2900 });
      run(el.querySelector(".intro-ring"), [
        { opacity: 0, transform: "scale(0.25)" },
        { opacity: 0.9, transform: "scale(0.4)", offset: 0.08 },
        { opacity: 0, transform: "scale(4.4)" },
      ], { delay: BLOOM - 40, duration: 1150, easing: "cubic-bezier(0.16, 0.84, 0.3, 1)" });
      run(el.querySelector(".intro-flash"), [{ opacity: 0 }, { opacity: 0.42, offset: 0.25 }, { opacity: 0 }], {
        delay: BLOOM - 60,
        duration: 760,
        easing: "ease-out",
      });

      // petals: drift in toward the light, then thrown outward in a spiral
      el.querySelectorAll<HTMLElement>(".intro-petal").forEach((node, i) => {
        const p = petals[i];
        const pos = (r: number, a: number) => `translate(${Math.cos(a) * r}px, ${Math.sin(a) * r}px)`;
        const rIn = mid * (1 - p.near) + 34;
        const total = 2900 - p.delay;
        const bloomAt = (BLOOM - p.delay) / total;
        run(node, [
          { transform: `${pos(far, p.angle)} rotate(0deg)`, opacity: 0, easing: "cubic-bezier(0.25, 0.6, 0.3, 1)" },
          { opacity: 0.95, offset: bloomAt * 0.35 },
          { transform: `${pos(rIn, p.angle + 0.55)} rotate(${p.spin * 0.45}deg)`, opacity: 1, offset: bloomAt, easing: "cubic-bezier(0.12, 0.85, 0.3, 1)" },
          { transform: `${pos(far * 1.05, p.angle + 1.5)} rotate(${p.spin}deg)`, opacity: 0 },
        ], { delay: p.delay, duration: total });
      });

      // the logo comes out of the glow
      run(img, [
        { opacity: 0, transform: "scale(0.8)", filter: "blur(16px) brightness(2.2) drop-shadow(0 0 30px rgba(207,160,255,0.9))" },
        { opacity: 1, transform: "scale(1)", filter: "blur(0px) brightness(1) drop-shadow(0 0 18px rgba(207,160,255,0.75))" },
      ], { delay: LOGO, duration: 950, easing: "cubic-bezier(0.22, 1, 0.36, 1)" });

      at(120, () => sounds.play("swipe"));
      at(BLOOM - 80, () => sounds.play("up"));
      at(FLY, fly);
    }

    const skip = (e: Event) => {
      e.preventDefault();
      fly();
    };
    el.addEventListener("pointerdown", skip);
    window.addEventListener("keydown", skip);
    return () => {
      el.removeEventListener("pointerdown", skip);
      window.removeEventListener("keydown", skip);
      timers.forEach((t) => window.clearTimeout(t));
      anims.forEach((a) => a.cancel());
    };
  }, [petals]);

  return (
    <div className="intro" ref={root} aria-hidden="true">
      <div className="intro-veil" />
      <div className="intro-stage">
        <div className="intro-flash" />
        <div className="intro-orb" />
        <div className="intro-ring" />
        <div className="intro-core" />
        {petals.map((p, i) => (
          <span
            key={i}
            className={`intro-petal ${p.blur ? "soft" : ""}`}
            style={{
              width: p.size,
              height: p.size * 0.62,
              marginLeft: -p.size / 2,
              marginTop: (-p.size * 0.62) / 2,
              filter: p.hue > 0.6 ? "hue-rotate(-14deg) brightness(0.8)" : undefined,
            }}
          />
        ))}
      </div>
      <img ref={logoRef} className="intro-logo" src={logo} alt="" draggable={false} />
    </div>
  );
}
