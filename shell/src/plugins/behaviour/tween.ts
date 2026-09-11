/**
 * Numeric count-up across renders.
 *
 * A plugin render replaces its DOM, so a changed number normally jumps to
 * the new value. Elements marked `data-sb-tween` count instead: the shell
 * remembers the last shown value per tile surface (the shell-owned
 * `data-sb-scope` on the render wrapper) and animates the text from the old
 * to the new value over `--sb-dur-slow`. Only the FIRST plain number in the
 * text animates; its prefix, suffix, decimal count and `,`/`.` separator
 * come from the newly rendered text (deliberately simple — keep thousands
 * grouping out of a tweened value). First render and reduced motion show
 * the target immediately.
 */
import { onSync, queryAll } from "./delegate";

const NUMBER = /-?\d+(?:[.,]\d+)?/;
const FALLBACK_MS = 240;
const MAX_DECIMALS = 20;

/** Last actually painted value per stable element key. */
const lastValue = new Map<string, number>();
const revisions = new Map<string, number>();
let nextRevision = 0;

/** Releases value memory when a tile surface is unmounted. */
export function clearTweenMemory(scope: string): void {
  const prefix = `${scope}#`;
  for (const key of lastValue.keys()) {
    if (key.startsWith(prefix)) {
      lastValue.delete(key);
      revisions.delete(key);
    }
  }
}

interface TweenTarget {
  readonly prefix: string;
  readonly suffix: string;
  readonly value: number;
  readonly decimals: number;
  readonly separator: string;
}

function parseTarget(text: string): TweenTarget | null {
  const match = NUMBER.exec(text);
  if (match === null) return null;
  const raw = match[0];
  const separatorIndex = raw.search(/[.,]/);
  const value = Number(raw.replace(",", "."));
  const decimals = separatorIndex === -1 ? 0 : raw.length - separatorIndex - 1;
  if (!Number.isFinite(value) || decimals > MAX_DECIMALS) return null;
  return {
    prefix: text.slice(0, match.index),
    suffix: text.slice(match.index + raw.length),
    value,
    decimals,
    separator: raw.includes(",") ? "," : ".",
  };
}

function format(target: TweenTarget, value: number): string {
  const number = value.toFixed(target.decimals).replace(".", target.separator);
  return `${target.prefix}${number}${target.suffix}`;
}

/** The kit's slow duration, read live so themes can retune motion. */
function durationMs(el: HTMLElement): number {
  const raw = getComputedStyle(el).getPropertyValue("--sb-dur-slow").trim();
  const value = Number.parseFloat(raw);
  if (!Number.isFinite(value) || value < 0) return FALLBACK_MS;
  if (raw.endsWith("ms")) return value;
  return raw.endsWith("s") ? value * 1000 : value;
}

function tween(
  el: HTMLElement,
  key: string,
  revision: number,
  from: number,
  finalText: string,
): void {
  const target = parseTarget(finalText);
  if (target === null) return;
  const total = durationMs(el);
  if (total === 0) {
    el.textContent = finalText;
    lastValue.set(key, target.value);
    return;
  }
  const start = performance.now();
  const frame = (now: number) => {
    if (revisions.get(key) !== revision) return;
    const t = Math.min((now - start) / total, 1);
    if (t >= 1) {
      // The exact rendered text, not a re-format of it.
      el.textContent = finalText;
      lastValue.set(key, target.value);
      return;
    }
    const eased = 1 - (1 - t) ** 3;
    const shown = from + (target.value - from) * eased;
    el.textContent = format(target, shown);
    lastValue.set(key, shown);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

onSync((root) => {
  // No scope (a surface without value memory, tests) → targets stand as-is.
  const scope = root instanceof HTMLElement ? root.dataset.sbScope : undefined;
  if (scope === undefined) return;
  const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const elements = queryAll(root, "[data-sb-tween]");
  const named = elements
    .map((el) => el.dataset.sbKey?.trim() ?? "")
    .filter((key) => key !== "");
  const unique = new Set(named);
  const active = new Set<string>();
  for (const el of elements) {
    const name = el.dataset.sbKey?.trim() ?? "";
    const key =
      name !== "" && unique.size === named.length
        ? `${scope}#key:${name}`
        : elements.length === 1
          ? `${scope}#single`
          : undefined;
    const text = el.textContent;
    const target = parseTarget(text);
    if (target === null || key === undefined) continue;
    active.add(key);
    const previous = lastValue.get(key);
    const revision = ++nextRevision;
    revisions.set(key, revision);
    if (previous === undefined || previous === target.value || still) {
      lastValue.set(key, target.value);
      continue;
    }
    // Paint the old value synchronously — the freshly rendered target must
    // not flash for a frame before the count starts.
    el.textContent = format(target, previous);
    tween(el, key, revision, previous, text);
  }
  const prefix = `${scope}#`;
  for (const key of lastValue.keys()) {
    if (key.startsWith(prefix) && !active.has(key)) {
      lastValue.delete(key);
      revisions.delete(key);
    }
  }
});
