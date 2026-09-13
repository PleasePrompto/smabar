import { invoke } from "@tauri-apps/api/core";

import { useSmabar } from "../store/bar";
import { reportError } from "./log";

/** Mirrors `smabar_core::platform::Rect` (logical/CSS pixels). */
interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 1px slack so antialiased region edges stay clickable after rounding. */
const PADDING = 1;

/** Set while a screenshot is being staged: the capture stage moves and
 *  unclips DOM the shape is measured from, and those rects must not reach
 *  the native input region. */
let paused = false;
let requestSync: (() => void) | null = null;
let requestForcedBarSync: (() => void) | null = null;

export function setInputShapePaused(value: boolean): void {
  paused = value;
  if (!value) requestSync?.();
}

export function requestBarGeometry(): void {
  requestForcedBarSync?.();
}

/**
 * The dock's bounds minus a pre-rendered, hidden solo row: the row keeps
 * its layout so the window never resizes, but neither the reservation nor
 * the input shape may claim that transparent space.
 */
function visibleDockRect(dock: Element): DOMRect {
  if (dock.querySelector("[data-bar-hidden]") === null) {
    return dock.getBoundingClientRect();
  }
  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;
  for (const row of dock.querySelectorAll(
    "[data-bar-root]:not([data-bar-hidden])",
  )) {
    const r = row.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) continue;
    left = Math.min(left, r.left);
    top = Math.min(top, r.top);
    right = Math.max(right, r.right);
    bottom = Math.max(bottom, r.bottom);
  }
  if (!Number.isFinite(left)) return dock.getBoundingClientRect();
  return new DOMRect(left, top, right - left, bottom - top);
}

export function collectRects(): Rect[] {
  const rects: Rect[] = [];
  const store = useSmabar.getState();
  for (const el of document.querySelectorAll("[data-input-region]")) {
    // The native autohide window moves as one static surface. Its wrapper is
    // the revealed input region; the permanent edge strip is used while it
    // is retracted.
    if (
      store.layout.behavior === "autohide" &&
      el.hasAttribute("data-bar-root")
    ) {
      continue;
    }
    const r = el.hasAttribute("data-bar-dock")
      ? visibleDockRect(el)
      : el.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) continue;
    rects.push({
      x: Math.floor(r.left) - PADDING,
      y: Math.floor(r.top) - PADDING,
      w: Math.ceil(r.width) + 2 * PADDING,
      h: Math.ceil(r.height) + 2 * PADDING,
    });
  }
  // A reorder drag claims the whole bar window for its duration: outside the
  // shape the window receives no motion events at all, so a tile dragged
  // past the bar row would freeze mid-drag (pointer capture cannot help —
  // the events never reach the window).
  if (store.reordering) {
    rects.push({ x: 0, y: 0, w: window.innerWidth, h: window.innerHeight });
  }
  return rects;
}

interface BarGeometry {
  position: "top" | "bottom";
  behavior: "reserve" | "float" | "autohide";
  rect: Rect;
  surface: { width: number; height: number };
}

/** Natural local bounds for the native bar window and its reservation. */
export function collectBarGeometry(): BarGeometry | null {
  const { position, behavior } = useSmabar.getState().layout;
  const dock = document.querySelector<HTMLElement>("[data-bar-dock]");
  if (dock === null) return null;
  const dockRect = dock.getBoundingClientRect();
  if (dockRect.width <= 0 || dockRect.height <= 0) return null;
  let left = dockRect.left;
  let top = dockRect.top;
  let right = dockRect.right;
  let bottom = dockRect.bottom;
  for (const element of document.querySelectorAll<HTMLElement>(
    "[data-shortcut-zone]",
  )) {
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) continue;
    left = Math.min(left, rect.left);
    top = Math.min(top, rect.top);
    right = Math.max(right, rect.right);
    bottom = Math.max(bottom, rect.bottom);
  }
  const workAreaWidth = Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue(
      "--sb-work-area-width",
    ),
  );
  const maxWidth = Number.isFinite(workAreaWidth)
    ? Math.max(1, Math.floor(workAreaWidth))
    : window.innerWidth;
  const workAreaHeight = Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue(
      "--sb-work-area-height",
    ),
  );
  const maxHeight = Number.isFinite(workAreaHeight)
    ? Math.max(1, Math.floor(workAreaHeight))
    : window.innerHeight;
  const naturalWidth = Math.ceil(right - left);
  const surfaceWidth =
    useSmabar.getState().layout.width === "full"
      ? maxWidth
      : Math.min(maxWidth, naturalWidth);
  const surfaceHeight = Math.min(
    maxHeight,
    Math.max(
      1,
      Math.ceil(position === "top" ? bottom : window.innerHeight - top),
    ),
  );
  // The reservation follows the visible rows; the surface keeps the room
  // for a hidden one, so showing it never resizes the window.
  const reserved = visibleDockRect(dock);
  return {
    position,
    behavior,
    rect: {
      x: Math.floor(reserved.left),
      y: Math.floor(reserved.top),
      w: Math.ceil(reserved.width),
      h: Math.ceil(reserved.height),
    },
    surface: { width: surfaceWidth, height: surfaceHeight },
  };
}

/**
 * Streams the UI's clickable regions to the Tauri core, which turns them
 * into the X11 input shape — clicks outside fall through to the desktop.
 *
 * Resize and DOM observers coalesce layout changes into one microtask.
 * Hidden Wayland surfaces may stop repainting, but their geometry and input
 * region must still update before they can be revealed again.
 */
export function initInputShape(): void {
  let lastSignature = "";
  let lastBarSignature = "";
  let scheduled = false;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  // Static plugin content may never trigger another store update. Retry
  // failed native geometry at most once per second, using fresh measurements.
  const retry = (): void => {
    if (retryTimer !== undefined) return;
    retryTimer = setTimeout(() => {
      retryTimer = undefined;
      schedule();
    }, 1_000);
  };
  const sync = (): void => {
    scheduled = false;
    if (paused) return;
    const rects = collectRects();
    const signature = JSON.stringify(rects);
    if (signature !== lastSignature) {
      lastSignature = signature;
      void invoke("set_input_shape", { rects }).catch((error: unknown) => {
        // Reset so the next update retries; surface the failure without
        // console.* (a permanently dead command would freeze click behavior).
        lastSignature = "";
        reportError(error);
        retry();
      });
    }
    reportBarGeometry();
  };
  const schedule = (): void => {
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(sync);
  };
  requestSync = schedule;
  requestForcedBarSync = () => {
    lastBarSignature = "";
    schedule();
  };

  // Reserve reports the union of every bar row. Float/autohide send a stable
  // zero rect so the core clears the native reservation. Overlay rows carry
  // no [data-bar-root] on purpose.
  const reportBarGeometry = (): void => {
    const geometry = collectBarGeometry();
    if (geometry === null) return;
    const { position, behavior, rect, surface } = geometry;
    const signature = `${position}|${behavior}|${JSON.stringify(rect)}|${JSON.stringify(surface)}`;
    if (signature === lastBarSignature) return;
    lastBarSignature = signature;
    void invoke("set_bar_geometry", { position, rect, surface }).catch(
      (error: unknown) => {
        lastBarSignature = "";
        reportError(error);
        retry();
      },
    );
  };

  const resizeObserver = new ResizeObserver(schedule);
  const observed = new WeakSet<Element>();
  const observeGeometry = (): void => {
    for (const element of document.querySelectorAll(
      "[data-bar-dock], [data-input-region], [data-shortcut-zone]",
    )) {
      if (observed.has(element)) continue;
      observed.add(element);
      resizeObserver.observe(element);
    }
  };
  const mutationObserver = new MutationObserver(() => {
    observeGeometry();
    schedule();
  });
  mutationObserver.observe(document.body, {
    attributes: true,
    childList: true,
    subtree: true,
  });
  observeGeometry();
  window.addEventListener("resize", schedule);
  useSmabar.subscribe(schedule);
  schedule();
}
