import { useEffect, useState } from "react";

import { uiLog } from "./log";
import type { SurfaceRole } from "./surface";

export type MemoryProbeMode =
  "observe" | "no-events" | "no-state" | "no-dom" | "no-clocks";

// Leave room for timer/wall-clock rounding at uiLog's dedupe boundary.
const SAMPLE_INTERVAL_MS = 31_000;
const STATE_WARMUP_MS = 30_000;
let mode: MemoryProbeMode | null = null;
let timer: ReturnType<typeof setInterval> | undefined;
let started = 0;

function emptyCounters() {
  return {
    liveEvents: 0,
    liveHtmlUnits: 0,
    snapshotEvents: 0,
    snapshotHtmlUnits: 0,
    domCommits: 0,
    committedHtmlUnits: 0,
    suppressedStateUpdates: 0,
    suppressedDomUpdates: 0,
    clockStarts: 0,
  };
}

let counters = emptyCounters();

/** Includes open plugin shadow roots, without retaining any DOM references. */
function elementCount(root: ParentNode): number {
  const elements = root.querySelectorAll("*");
  let count = elements.length;
  for (const element of elements) {
    if (element.shadowRoot !== null) count += elementCount(element.shadowRoot);
  }
  return count;
}

/** Process-only diagnostic state; absent in ordinary launches. */
export function initMemoryProbe(
  next: MemoryProbeMode | null | undefined,
  role: SurfaceRole,
): void {
  clearInterval(timer);
  timer = undefined;
  mode = next ?? null;
  counters = emptyCounters();
  if (mode === null) return;
  started = performance.now();
  let since = started;
  timer = setInterval(() => {
    const now = performance.now();
    uiLog("info", "memory probe shell sample", {
      fields: {
        role,
        mode,
        elapsedMs: Math.round(now - since),
        ...counters,
        domElements: elementCount(document),
        visibility: document.visibilityState,
      },
    });
    counters = emptyCounters();
    since = now;
  }, SAMPLE_INTERVAL_MS);
}

export function memoryProbeIs(expected: MemoryProbeMode): boolean {
  return mode === expected;
}

/** Call after counting a persistent live update, before reading frontend state. */
export function suppressMemoryStateUpdate(): boolean {
  if (mode !== "no-state" || performance.now() - started < STATE_WARMUP_MS)
    return false;
  counters.suppressedStateUpdates += 1;
  return true;
}

/** Lengths are UTF-16 code units, not bytes or browser heap measurements. */
export function recordMemoryUi(
  units: number,
  source: "live" | "snapshot",
): void {
  if (mode === null) return;
  if (source === "live") {
    counters.liveEvents += 1;
    counters.liveHtmlUnits += units;
  } else {
    counters.snapshotEvents += 1;
    counters.snapshotHtmlUnits += units;
  }
}

export function recordMemoryDomCommit(units: number): void {
  if (mode === null) return;
  counters.domCommits += 1;
  counters.committedHtmlUnits += units;
}

export function recordMemoryClockStarts(root: ParentNode): void {
  if (mode === null) return;
  counters.clockStarts += root.querySelectorAll(
    "[data-clock], [data-clock-text]",
  ).length;
}

/** Freeze this mounted host's first HTML; keep its original clock cleanup alive. */
export function useMemoryProbeHtml(html: string): string {
  const [frozenHtml] = useState(() => (mode === "no-dom" ? html : null));
  useEffect(() => {
    if (frozenHtml !== null && html !== frozenHtml) {
      counters.suppressedDomUpdates += 1;
    }
  }, [frozenHtml, html]);
  return frozenHtml ?? html;
}
