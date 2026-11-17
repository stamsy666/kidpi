import { useEffect, useRef, useState } from "react";
import { sounds } from "./sounds";

/** The strategy picker in Settings: a custom dropdown with its own scrollbar. */
export default function StrategySelect({
  value,
  options,
  onChange,
}: {
  value: string | null;
  options: string[];
  onChange: (name: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);

  const close = () => {
    if (!open || closing) return;
    setClosing(true);
    window.setTimeout(() => {
      setOpen(false);
      setClosing(false);
    }, 160);
  };

  const toggle = () => {
    if (open) close();
    else {
      sounds.play("swipe");
      setOpen(true);
    }
  };

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrap.current && !wrap.current.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  });

  // open with the current strategy in view
  useEffect(() => {
    if (!open) return;
    const l = list.current;
    const sel = l?.querySelector<HTMLElement>('[aria-selected="true"]');
    if (l && sel) l.scrollTop = Math.max(0, sel.offsetTop - l.clientHeight / 2 + sel.offsetHeight / 2);
  }, [open]);

  return (
    <div className="select" ref={wrap}>
      <button
        type="button"
        className={`select-btn ${open ? "open" : ""}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`Стратегия: ${value ?? "не выбрана"}`}
        onClick={toggle}
      >
        <span>{value ?? "Не выбрана"}</span>
        <svg className="chev" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>
      {open && (
        <div className={`popover ${closing ? "closing" : ""}`}>
          <div className="popover-list scroll" role="listbox" aria-label="Стратегии" ref={list}>
            {options.map((name) => {
              const sel = name === value;
              return (
                <button
                  key={name}
                  type="button"
                  role="option"
                  aria-selected={sel}
                  className={`option ${sel ? "sel" : ""}`}
                  onClick={() => {
                    if (!sel) {
                      sounds.play("select");
                      onChange(name);
                    }
                    close();
                  }}
                >
                  <span>{name}</span>
                  {sel && (
                    <svg className="check" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                      <path d="M20 6 9 17l-5-5" />
                    </svg>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
