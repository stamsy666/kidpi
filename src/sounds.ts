// UI sounds: SND01 "sine" pack (snd.dev).
import button from "./assets/sounds/button.wav";
import tap from "./assets/sounds/tap_01.wav";
import toggleOn from "./assets/sounds/toggle_on.wav";
import toggleOff from "./assets/sounds/toggle_off.wav";
import up from "./assets/sounds/transition_up.wav";
import down from "./assets/sounds/transition_down.wav";
import swipe from "./assets/sounds/swipe_01.wav";
import select from "./assets/sounds/select.wav";
import success from "./assets/sounds/celebration.wav";
import loop from "./assets/sounds/progress_loop.wav";
import notify from "./assets/sounds/notification.wav";
import caution from "./assets/sounds/caution.wav";
import disabled from "./assets/sounds/disabled.wav";

const SRC = {
  button: [button, 0.6],
  tap: [tap, 0.5],
  toggleOn: [toggleOn, 0.6],
  toggleOff: [toggleOff, 0.6],
  up: [up, 0.5],
  down: [down, 0.5],
  swipe: [swipe, 0.5],
  select: [select, 0.6],
  success: [success, 0.55],
  loop: [loop, 0.22],
  notify: [notify, 0.6],
  caution: [caution, 0.6],
  disabled: [disabled, 0.6],
} as const;

export type Sound = keyof typeof SRC;

class Sounds {
  enabled = true;
  private cache = new Map<Sound, HTMLAudioElement>();
  private looping: HTMLAudioElement | null = null;

  private base(name: Sound) {
    let a = this.cache.get(name);
    if (!a) {
      a = new Audio(SRC[name][0]);
      a.preload = "auto";
      a.volume = SRC[name][1];
      this.cache.set(name, a);
    }
    return a;
  }

  preload() {
    (Object.keys(SRC) as Sound[]).forEach((n) => this.base(n));
  }

  play(name: Sound) {
    if (!this.enabled) return;
    const b = this.base(name);
    const a = b.cloneNode() as HTMLAudioElement;
    a.volume = b.volume;
    a.play().catch(() => {});
  }

  loopStart() {
    if (!this.enabled || this.looping) return;
    const a = this.base("loop");
    a.loop = true;
    a.currentTime = 0;
    a.play().catch(() => {});
    this.looping = a;
  }

  loopStop() {
    if (!this.looping) return;
    this.looping.pause();
    this.looping.currentTime = 0;
    this.looping = null;
  }
}

export const sounds = new Sounds();
