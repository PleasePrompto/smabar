/**
 * Live countdown to a target time.
 *
 * A plugin could render the remaining time itself, but that means an RPC
 * round trip per second for a number the browser can compute. This ticks
 * locally: the plugin renders the target once and the seconds keep running
 * even while the plugin is busy.
 *
 * One interval serves every countdown in the window rather than one each —
 * a plugin re-renders on its own schedule, and per-element timers would have
 * to be torn down and rebuilt on every render.
 */
import { onSync, queryAll } from "./delegate";

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** The countdown roots currently on screen, refreshed on every render. */
let roots: HTMLElement[] = [];
let ticker: number | null = null;

interface Remaining {
  readonly days: number;
  readonly hours: number;
  readonly minutes: number;
  readonly seconds: number;
}

/**
 * Time left, recomputed from the clock rather than counted down, so a
 * throttled or suspended interval cannot make it drift.
 */
function remaining(target: number): Remaining {
  const diff = target - Date.now();
  if (diff <= 0) return { days: 0, hours: 0, minutes: 0, seconds: 0 };
  return {
    days: Math.floor(diff / DAY),
    hours: Math.floor(diff / HOUR) % 24,
    minutes: Math.floor(diff / MINUTE) % 60,
    seconds: Math.floor(diff / SECOND) % 60,
  };
}

/** Writes the parts a root declares, touching only what changed. */
function paint(root: HTMLElement, parts: Remaining): void {
  for (const slot of queryAll(root, "[data-sb-countdown-part]")) {
    const key = slot.dataset.sbCountdownPart;
    if (
      key !== "days" &&
      key !== "hours" &&
      key !== "minutes" &&
      key !== "seconds"
    ) {
      continue;
    }
    const text = String(parts[key]).padStart(2, "0");
    if (slot.textContent !== text) slot.textContent = text;
  }
}

/** Marks a countdown finished and reveals its done message. */
function finish(root: HTMLElement): void {
  root.classList.add("is-done");
  const done = root.querySelector<HTMLElement>("[data-sb-countdown-done]");
  if (done !== null) done.hidden = false;
}

/** Updates one root. Returns false when it has nothing left to count. */
function step(root: HTMLElement): boolean {
  const target = Date.parse(root.dataset.sbCountdown ?? "");
  if (Number.isNaN(target)) return false;
  const parts = remaining(target);
  paint(root, parts);
  if (target > Date.now()) return true;
  finish(root);
  return false;
}

function tick(): void {
  roots = roots.filter((root) => root.isConnected && step(root));
  if (roots.length === 0 && ticker !== null) {
    clearInterval(ticker);
    ticker = null;
  }
}

onSync((root) => {
  const found = queryAll(root, "[data-sb-countdown]");
  // Roots from a replaced render are dropped by the isConnected filter in
  // tick(); collect the live ones and paint them immediately so a countdown
  // is never blank for up to a second after a render.
  for (const element of found) {
    if (step(element) && !roots.includes(element)) roots.push(element);
  }
  if (roots.length > 0 && ticker === null) {
    ticker = window.setInterval(tick, SECOND);
  }
});
