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
let role: SurfaceRole = "bar";
let session = 0;
let nextHostInstance = 0;
let activeHosts = 0;
let activeClockEnhancers = 0;

function emptyCounters() {
  return {
    liveBatches: 0,
    liveEvents: 0,
    liveHtmlUnits: 0,
    snapshotEvents: 0,
    snapshotHtmlUnits: 0,
    domCommits: 0,
    committedHtmlUnits: 0,
    suppressedStateUpdates: 0,
    suppressedDomUpdates: 0,
    clockStarts: 0,
    clockStops: 0,
    clockEnhancerStarts: 0,
    clockEnhancerStops: 0,
    hostMounts: 0,
    hostCleanups: 0,
  };
}

let counters = emptyCounters();

/** Includes open plugin shadow roots, without retaining any DOM references. */
function domCounts(root: ParentNode) {
  const elements = root.querySelectorAll("*");
  const counts = {
    domElements: elements.length,
    domInputs: root.querySelectorAll("input").length,
    domForms: root.querySelectorAll("form").length,
    domShadowRoots: root instanceof ShadowRoot ? 1 : 0,
  };
  for (const element of elements) {
    const shadow = element.shadowRoot;
    if (shadow === null) continue;
    const nested = domCounts(shadow);
    counts.domElements += nested.domElements;
    counts.domInputs += nested.domInputs;
    counts.domForms += nested.domForms;
    counts.domShadowRoots += nested.domShadowRoots;
  }
  return counts;
}

/** Process-only diagnostic state; absent in ordinary launches. */
export function initMemoryProbe(
  next: MemoryProbeMode | null | undefined,
  nextRole: SurfaceRole,
): void {
  clearInterval(timer);
  timer = undefined;
  mode = next ?? null;
  role = nextRole;
  session += 1;
  activeHosts = 0;
  activeClockEnhancers = 0;
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
        session,
        elapsedMs: Math.round(now - since),
        ...counters,
        activeHosts,
        activeClockEnhancers,
        ...domCounts(document),
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

/** One received `plugin-ui-*` event; its elements count as live events. */
export function recordMemoryBatch(): void {
  if (mode === null) return;
  counters.liveBatches += 1;
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

interface MemoryHostIdentity {
  pluginId: string;
  tileId: string;
  target: "tile" | "flyout" | "popup";
  scope: string | undefined;
  generation: number | undefined;
  htmlUnits: number;
}

/** One layout-effect setup/cleanup pair. Retains only identity and numbers. */
export function beginMemoryHost(identity: MemoryHostIdentity) {
  if (mode === null) return undefined;
  const hostSession = session;
  const instanceId = ++nextHostInstance;
  const renderStarted = performance.now();
  let mounted = false;
  let disposed = false;
  let clockEnhancers = 0;
  let clockElements = 0;
  let mountedCounts: ReturnType<typeof domCounts> | undefined;
  const log = (phase: "mounted" | "cleanup") => {
    uiLog("info", `memory probe host ${phase}`, {
      deduplicate: false,
      fields: {
        role,
        mode,
        session: hostSession,
        ...identity,
        scope: identity.scope ?? null,
        generation: identity.generation ?? null,
        instanceId,
        sinceStartMs: Math.round(performance.now() - started),
        durationMs: Math.round(performance.now() - renderStarted),
        ...mountedCounts,
        clockElements,
        clockEnhancers,
        activeHosts,
        activeClockEnhancers,
      },
    });
  };
  return {
    mounted(root: ShadowRoot, clocksStarted: boolean): void {
      if (disposed || mounted || hostSession !== session) return;
      mountedCounts = domCounts(root);
      clockEnhancers = clocksStarted ? 1 : 0;
      clockElements = clocksStarted
        ? root.querySelectorAll("[data-clock], [data-clock-text]").length
        : 0;
      mounted = true;
      activeHosts += 1;
      activeClockEnhancers += clockEnhancers;
      counters.hostMounts += 1;
      counters.clockStarts += clockElements;
      counters.clockEnhancerStarts += clockEnhancers;
      log("mounted");
    },
    cleanup(): void {
      if (disposed) return;
      disposed = true;
      if (!mounted || hostSession !== session) return;
      activeHosts -= 1;
      activeClockEnhancers -= clockEnhancers;
      counters.hostCleanups += 1;
      counters.clockStops += clockElements;
      counters.clockEnhancerStops += clockEnhancers;
      log("cleanup");
    },
  };
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
