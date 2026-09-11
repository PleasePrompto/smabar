import { useEffect, useRef, useState, type RefObject } from "react";

import { reportError } from "../../ipc/log";
import { setBarRevealed } from "../../ipc/surface";
import { useSmabar, type LayoutBehavior } from "../../store/bar";

/**
 * Grace period before an autohide bar retracts after the pointer left it.
 * Every established dock has one (dash-to-dock hide-delay 200ms, KWin
 * electric-border cooldown 350ms, Latte timerHide 700ms): without it a
 * single stray "outside" sample — brushing the bar's edge, or the X11 input
 * shape changing under a resting pointer — retracts the bar mid-animation
 * and the transform target flips several times a second.
 */
export const AUTOHIDE_HIDE_DELAY_MS = 400;
const AUTOHIDE_VISIBLE_HEIGHT = 8;
const MOTION_FALLBACK_MS = 180;

function motionDurationMs(): number {
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return 0;
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue("--sb-dur-normal")
    .trim();
  const value = Number.parseFloat(raw);
  if (!Number.isFinite(value) || value < 0) return MOTION_FALLBACK_MS;
  return raw.endsWith("s") && !raw.endsWith("ms") ? value * 1_000 : value;
}

/** Client-space rectangle; a DOMRect satisfies it. */
export interface EdgeRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Pure visibility rule shared by the hook and its focused tests. */
export function autohideRevealed(
  behavior: LayoutBehavior,
  pointerInside: boolean,
  surfaceOpen: boolean,
): boolean {
  return behavior !== "autohide" || pointerInside || surfaceOpen;
}

/**
 * Does the pointer sit in one of the regions that keep the bar out?
 * Null rects (an unmounted element) never match. Native window movement
 * carries the dock out of the screen while leaving its input region intact.
 */
export function pointerInAutohideRegion(
  x: number,
  y: number,
  rects: (EdgeRect | null)[],
): boolean {
  return rects.some(
    (r) =>
      r !== null && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom,
  );
}

type PointerSample = (x: number, y: number) => void;

const externalPointer = new Set<PointerSample>();
let nativePointerInside = true;

/**
 * Feeds a pointer position that never reaches the DOM event stream.
 *
 * During an OS file drag X11 grabs the pointer: the webview receives no
 * mousemove at all, so the coordinate-based reveal rule below would never
 * see the drag approach the screen edge and an autohide bar would stay
 * retracted — with no way to drop anything on it (reported bug). Tauri's
 * `onDragDropEvent` reports the drag position instead, and `dragDrop.ts`
 * pushes it here in CSS pixels. A NaN sample means "position unknown" and
 * lets the grace period retract the bar, exactly like leaving the window.
 */
export function pushPointerSample(x: number, y: number): void {
  for (const sample of externalPointer) sample(x, y);
}

/** Native presence outranks cached DOM coordinates after a window moves. */
export function pushNativePointerSample(x: number, y: number): void {
  nativePointerInside = Number.isFinite(x) && Number.isFinite(y);
  // WebKit can keep its last CSS :hover match after Wayland stopped routing
  // pointer events to the input-shaped surface. Bar hover selectors use this
  // native state instead of trusting that stale browser match.
  document.documentElement.toggleAttribute(
    "data-sb-pointer-outside",
    !nativePointerInside,
  );
  pushPointerSample(x, y);
}

export function nativePointerIsInside(): boolean {
  return nativePointerInside;
}

export function subscribePointerSamples(sample: PointerSample): () => void {
  externalPointer.add(sample);
  return () => externalPointer.delete(sample);
}

export interface Autohide {
  revealed: boolean;
  /** The stable bar wrapper. */
  surfaceRef: RefObject<HTMLDivElement | null>;
  /** The edge activation strip. */
  hotzoneRef: RefObject<HTMLDivElement | null>;
  /** The gap between the revealed dock and its outer window edge. */
  edgeGapRef: RefObject<HTMLDivElement | null>;
}

/**
 * Reveal state of the autohide bar.
 *
 * "Inside" is decided from the pointer COORDINATES against the very rects
 * that make up the X11 input shape — never from which element the DOM
 * considers hovered. Both differ: the shortcut zone reserves headroom for
 * magnified tiles by overflowing the bar row by a few px, so the pointer can
 * leave the input shape while the DOM still hovers the zone. Outside the
 * shape the window stops receiving motion events, so an element-based rule
 * (mouseenter/mouseleave) freezes in that band: the bar hangs revealed and
 * the fisheye stays magnified.
 *
 * Reveal is immediate, hiding waits out {@link AUTOHIDE_HIDE_DELAY_MS} and
 * is cancelled by any sample back inside — the hysteresis every dock uses.
 *
 * Settings stays visible for design previews. Two drags also hold the bar: a tile being
 * reordered inside the bar, and an OS file drag over the window. Both hold
 * the pointer hostage (no mousemove arrives), and retracting the bar out
 * from under either one destroys the interaction in progress.
 */
export function useAutohide(
  behavior: LayoutBehavior,
  surfaceOpen: boolean,
): Autohide {
  const surfaceRef = useRef<HTMLDivElement | null>(null);
  const hotzoneRef = useRef<HTMLDivElement | null>(null);
  const edgeGapRef = useRef<HTMLDivElement | null>(null);
  const [pointerInside, setPointerInside] = useState(false);
  const reordering = useSmabar((s) => s.reordering);
  const fileDrag = useSmabar((s) => s.fileDrag);
  const settingsOpen = useSmabar((s) => s.settingsOpen);

  useEffect(() => {
    if (behavior !== "autohide") return;
    let hideTimer = 0;
    let x = Number.NaN;
    let y = Number.NaN;

    const cancelHide = () => {
      if (hideTimer === 0) return;
      clearTimeout(hideTimer);
      hideTimer = 0;
    };
    const insideRegion = (): boolean => {
      return pointerInAutohideRegion(x, y, [
        hotzoneRef.current?.getBoundingClientRect() ?? null,
        edgeGapRef.current?.getBoundingClientRect() ?? null,
        surfaceRef.current?.getBoundingClientRect() ?? null,
      ]);
    };
    // A running grace period wins over later "outside" samples, so a pointer
    // resting outside cannot keep pushing the retraction away. It re-checks
    // the last sample against the settled geometry before it hides.
    const scheduleHide = () => {
      if (hideTimer !== 0) return;
      hideTimer = window.setTimeout(() => {
        hideTimer = 0;
        setPointerInside(insideRegion());
      }, AUTOHIDE_HIDE_DELAY_MS);
    };
    const evaluate = () => {
      if (insideRegion()) {
        cancelHide();
        setPointerInside(true);
      } else {
        scheduleHide();
      }
    };
    const sample = (sampleX: number, sampleY: number) => {
      x = sampleX;
      y = sampleY;
      // A hidden Wayland surface may receive no animation frames. The
      // pointer must start its native reveal without waiting for a repaint.
      evaluate();
    };
    const onMove = (event: MouseEvent) => {
      if (!nativePointerInside) return;
      sample(event.clientX, event.clientY);
    };
    // A null relatedTarget means the pointer left the document — under the
    // input shape that is also how leaving the window arrives. No further
    // coordinates follow, so drop the last sample (NaN never matches a
    // region) and let the grace period run out.
    const onOut = (event: MouseEvent) => {
      if (event.relatedTarget !== null) return;
      x = Number.NaN;
      y = Number.NaN;
      scheduleHide();
    };

    document.addEventListener("mousemove", onMove, { passive: true });
    document.addEventListener("mouseover", onMove, { passive: true });
    document.addEventListener("mouseout", onOut, { passive: true });
    const unsubscribePointerSamples = subscribePointerSamples(sample);
    return () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseover", onMove);
      document.removeEventListener("mouseout", onOut);
      unsubscribePointerSamples();
      cancelHide();
      // Leaving autohide drops the tracked pointer: a stale "inside" would
      // otherwise reveal the bar on the way back in until the pointer moves.
      setPointerInside(false);
    };
  }, [behavior]);

  const revealed = autohideRevealed(
    behavior,
    pointerInside,
    surfaceOpen || settingsOpen || reordering || fileDrag,
  );
  useEffect(() => {
    void setBarRevealed(
      revealed,
      motionDurationMs(),
      AUTOHIDE_VISIBLE_HEIGHT,
    ).catch(reportError);
  }, [revealed]);

  return {
    revealed,
    surfaceRef,
    hotzoneRef,
    edgeGapRef,
  };
}
