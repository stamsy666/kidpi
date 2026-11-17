import type { SVGProps } from "react";

const TRACK = "M12 7h5c2.76 0 5 2.24 5 5c0 2.76 -2.24 5 -5 5h-10c-2.76 0 -5 -2.24 -5 -5c0 -2.76 2.24 -5 5 -5Z";

type P = SVGProps<SVGSVGElement> & { size?: number };

export function SwitchOffToSwitchTransitionIcon({ size = 24, ...props }: P) {
  return (
    <svg width={size} height={size} xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" {...props}>
      <path fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d={TRACK} />
      <circle cx="17" cy="12" r="3" fill="currentColor">
        <animate fill="freeze" attributeName="cx" dur="0.2s" values="7;17" />
      </circle>
    </svg>
  );
}

export function SwitchToSwitchOffTransitionIcon({ size = 24, ...props }: P) {
  return (
    <svg width={size} height={size} xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" {...props}>
      <path fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d={TRACK} />
      <circle cx="7" cy="12" r="3" fill="currentColor">
        <animate fill="freeze" attributeName="cx" dur="0.2s" values="17;7" />
      </circle>
    </svg>
  );
}

/** On/off switch built from the two transition icons; the key remounts the SVG so the slide replays. */
export function Switch({
  on,
  label,
  onToggle,
  size = 44,
}: {
  on: boolean;
  label: string;
  onToggle: () => void;
  size?: number;
}) {
  return (
    <button type="button" role="switch" aria-checked={on} aria-label={label} className={`switch ${on ? "on" : "off"}`} onClick={onToggle}>
      {on ? (
        <SwitchOffToSwitchTransitionIcon key="on" size={size} aria-hidden="true" />
      ) : (
        <SwitchToSwitchOffTransitionIcon key="off" size={size} aria-hidden="true" />
      )}
    </button>
  );
}
