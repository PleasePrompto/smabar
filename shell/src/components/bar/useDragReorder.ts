import { useEffect, useRef, type RefObject } from "react";

import { useSmabar } from "../../store/bar";
import { closeFlyoutSurface } from "../../ipc/overlay";
import { reportError } from "../../ipc/log";
import { DRAG_MARKER } from "../../styles/layers";
import {
  autoScrollDelta,
  insertionIndex,
  LONG_PRESS_MS,
  PRESS_IDLE,
  pressHeld,
  pressMoved,
  reorderTarget,
  slotMarker,
  type ItemSpan,
  type PressState,
} from "../dragReorder";
import { applyShift, clearShift, setShift } from "../reorderShift";

/** Must match `.zone-drop-marker`'s width in bar.css. */
const MARKER_WIDTH_PX = 2;

export interface DragReorderOptions {
  /**
   * Item ids in render order. Only the length is read (as the guard that the
   * zone's element children are index-aligned with the model) — the drag
   * itself works in indices, which keeps the zones free of id plumbing.
   */
  items: readonly string[];
  /** Commits a finished drag; `from`/`to` are indices into {@link items}. */
  onReorder: (from: number, to: number) => void;
  /** Vertical headroom the zone reserves for magnified tiles (marker inset). */
  reserve?: number;
}

/** Index of the zone child containing `target`, or -1 when it is elsewhere. */
function childIndex(zone: Element, target: EventTarget | null): number {
  if (!(target instanceof Node)) return -1;
  let node: Node | null = target;
  while (node !== null && node.parentNode !== zone) node = node.parentNode;
  if (!(node instanceof Element)) return -1;
  return [...zone.children].indexOf(node);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * Long-press drag reordering for a bar zone, implemented ONCE on the zone
 * container instead of per tile: a single `pointerdown` listener resolves the
 * pressed item via the event's path, so the tile components stay plain
 * buttons and a zone with 40 pins still installs one listener.
 *
 * Interaction: hold a tile for {@link LONG_PRESS_MS} without moving more than
 * the press tolerance, then the tile lifts (semi-transparent, follows the
 * pointer) and an insertion marker shows where it lands. Moving during the
 * hold cancels it and the press stays an ordinary click — a launch must never
 * happen by accident, and neither must a drag.
 *
 * Geometry is measured ONCE at drag start and kept in the zone's scroll
 * content space, so a frame costs one `getBoundingClientRect` on the zone
 * plus a couple of style writes — no per-tile layout reads, no reflow, and
 * the numbers stay valid while the zone auto-scrolls under an overflowing
 * dock. The tiles the dragged one passes step aside as it goes
 * (`reorderShift.ts`, shared with the settings list), so the hole that opens
 * says where it lands and the marker only confirms it. That reflow is written
 * as `translate`, which is why it can coexist with the fisheye's `scale`
 * without either side knowing about the other.
 *
 * The dragged tile is the ORIGINAL element, translated in place — not a
 * cloned ghost. Plugin tiles render into shadow roots (`cloneNode` drops
 * those, leaving an empty ghost), and a portaled copy would have to be kept
 * in sync with live tile content for the whole drag.
 */
export function useDragReorder(
  ref: RefObject<HTMLElement | null>,
  options: DragReorderOptions,
): void {
  const latest = useRef(options);
  // No dependency array: the zone re-renders with fresh items and a fresh
  // commit closure, while the effect below installs its listeners once.
  useEffect(() => {
    latest.current = options;
  });

  const reserve = options.reserve ?? 0;

  useEffect(() => {
    const zone = ref.current;
    if (zone === null) return;

    let state: PressState = PRESS_IDLE;
    let pointerId = -1;
    let holdTimer = 0;
    let frame = 0;
    let x = 0;
    let y = 0;
    /** Item extents in scroll content space, measured at drag start. */
    let spans: ItemSpan[] = [];
    /** The zone's children, captured at drag start — see reorderShift.ts. */
    let items: HTMLElement[] = [];
    /** Distance from the dragged item's left edge to the grab point. */
    let grabOffset = 0;
    let insertAt = 0;
    let source: HTMLElement | null = null;
    let marker: HTMLElement | null = null;

    const update = (): void => {
      frame = 0;
      const current = state;
      if (current.phase !== "dragging" || source === null || marker === null) {
        return;
      }
      const span = spans[current.index];
      if (span === undefined) return;
      const zoneRect = zone.getBoundingClientRect();
      // Only an overflowing zone scrolls; scrollLeft is clamped by the DOM.
      const step =
        zone.scrollWidth - zone.clientWidth > 1
          ? autoScrollDelta(x, zoneRect.left, zoneRect.right)
          : 0;
      if (step !== 0) zone.scrollLeft += step;

      const scrollLeft = zone.scrollLeft;
      const width = span.end - span.start;
      const wanted = x - zoneRect.left + scrollLeft - grabOffset;
      const left = clamp(wanted, 0, Math.max(0, zone.scrollWidth - width));
      // `translate`, not `transform`: the tile's transform belongs to the
      // fisheye's scale, and two owners on one property means one of them has
      // to re-state the other's value every frame.
      setShift(source, left - span.start, "x");

      // The dragged tile's own center picks the slot — what the eye follows.
      // Read UNCLAMPED, so the first slot stays reachable: clamped, the tile's
      // centre lands exactly on the first tile's centre, and insertionIndex's
      // tie goes to the slot below.
      const next = insertionIndex(spans, wanted + width / 2);
      if (next !== insertAt) {
        insertAt = next;
        applyShift(items, spans, current.index, next, "x");
      }
      const markerX = clamp(
        zoneRect.left -
          scrollLeft +
          slotMarker(spans, current.index, insertAt, MARKER_WIDTH_PX),
        zoneRect.left,
        Math.max(zoneRect.left, zoneRect.right - MARKER_WIDTH_PX),
      );
      marker.style.transform = `translate3d(${markerX.toFixed(2)}px, ${(zoneRect.top + reserve).toFixed(2)}px, 0)`;
      marker.style.height = `${Math.max(0, zoneRect.height - 2 * reserve).toFixed(2)}px`;

      // A pointer resting in an edge band must keep scrolling without motion.
      if (step !== 0) frame = requestAnimationFrame(update);
    };

    const finish = (commit: boolean): void => {
      const current = state;
      state = PRESS_IDLE;
      if (holdTimer !== 0) {
        window.clearTimeout(holdTimer);
        holdTimer = 0;
      }
      if (frame !== 0) {
        cancelAnimationFrame(frame);
        frame = 0;
      }
      if (current.phase !== "dragging") return;

      marker?.remove();
      marker = null;
      // Attribute first, styles second: it is what arms the translate
      // transition, so clearing the shift while it is still set would slide
      // every tile back to where it came from as the new order arrives.
      zone.toggleAttribute("data-reordering", false);
      source?.classList.remove("zone-drag-source");
      source = null;
      clearShift(items);
      items = [];
      if (zone.hasPointerCapture(pointerId)) {
        zone.releasePointerCapture(pointerId);
      }
      useSmabar.getState().setReordering(false);

      if (!commit) return;
      const to = reorderTarget(current.index, insertAt);
      if (to !== current.index) latest.current.onReorder(current.index, to);
    };

    const beginDrag = (): void => {
      const current = state;
      if (current.phase !== "pending") return;
      const child = zone.children[current.index];
      if (!(child instanceof HTMLElement)) {
        state = PRESS_IDLE;
        return;
      }
      // Suspend the fisheye BEFORE measuring: a magnified tile reports its
      // SCALED rect. bar.css also drops the tile transition under this
      // attribute, so clearing --magnify snaps instead of animating away
      // while the rects below are being read.
      zone.toggleAttribute("data-reordering", true);
      for (const tile of zone.querySelectorAll<HTMLElement>(".shortcut-tile")) {
        tile.style.removeProperty("--magnify");
      }

      const children = [...zone.children];
      // The shift writes inline styles, so the array it gets must stay
      // index-aligned with `spans` — filtering would silently slide every
      // index by one, so a zone rendering anything else refuses the drag.
      if (
        !children.every(
          (element): element is HTMLElement => element instanceof HTMLElement,
        )
      ) {
        zone.toggleAttribute("data-reordering", false);
        state = PRESS_IDLE;
        return;
      }
      const zoneRect = zone.getBoundingClientRect();
      const scrollLeft = zone.scrollLeft;
      spans = children.map((element): ItemSpan => {
        const rect = element.getBoundingClientRect();
        return {
          start: rect.left - zoneRect.left + scrollLeft,
          end: rect.right - zoneRect.left + scrollLeft,
        };
      });
      const span = spans[current.index];
      if (span === undefined) {
        zone.toggleAttribute("data-reordering", false);
        state = PRESS_IDLE;
        return;
      }
      items = children;

      grabOffset = x - zoneRect.left + scrollLeft - span.start;
      insertAt = current.index;
      source = child;
      source.classList.add("zone-drag-source");
      marker = document.createElement("div");
      marker.className = "zone-drop-marker";
      marker.style.zIndex = DRAG_MARKER;
      marker.setAttribute("aria-hidden", "true");
      // Placed before it enters the document: the marker now transitions with
      // the tiles, and a transition never runs on an element's initial style,
      // so it appears where it belongs instead of flying in from the corner.
      marker.style.transform = `translate3d(${(
        zoneRect.left -
        scrollLeft +
        slotMarker(spans, current.index, current.index, MARKER_WIDTH_PX)
      ).toFixed(2)}px, ${(zoneRect.top + reserve).toFixed(2)}px, 0)`;
      marker.style.height = `${Math.max(0, zoneRect.height - 2 * reserve).toFixed(2)}px`;
      document.body.append(marker);

      state = pressHeld(current);
      zone.setPointerCapture(pointerId);
      const store = useSmabar.getState();
      // A flyout is anchored to a measured trigger rect that the drag is
      // about to invalidate; the store flag holds an autohide bar out and
      // widens the input shape for the whole drag.
      store.setReordering(true);
      // The native overlay has its own lifecycle; clearing bar state alone
      // leaves its window visible and able to intercept the drop.
      void closeFlyoutSurface().catch(reportError);
      update();
    };

    /**
     * The press ended on the tile, so the browser dispatches a click next.
     * It must not launch the app that was just moved.
     */
    const suppressNextClick = (): void => {
      const stop = (event: Event): void => {
        event.preventDefault();
        event.stopPropagation();
        document.removeEventListener("click", stop, true);
      };
      document.addEventListener("click", stop, true);
      // Not every drag ends in a click — drop the guard on the next task so
      // it can never swallow an unrelated one.
      window.setTimeout(() => {
        document.removeEventListener("click", stop, true);
      }, 0);
    };

    const onPointerDown = (event: PointerEvent): void => {
      if (state.phase !== "idle" || event.button !== 0 || !event.isPrimary) {
        return;
      }
      const items = latest.current.items;
      // Nothing to reorder, or the zone renders something other than one
      // element per item (the empty-state hint) — leave the press alone.
      if (items.length < 2 || zone.children.length !== items.length) return;
      const index = childIndex(zone, event.target);
      if (index < 0) return;
      pointerId = event.pointerId;
      x = event.clientX;
      y = event.clientY;
      state = { phase: "pending", index, x, y };
      holdTimer = window.setTimeout(() => {
        holdTimer = 0;
        beginDrag();
      }, LONG_PRESS_MS);
    };

    const onPointerMove = (event: PointerEvent): void => {
      if (state.phase === "idle" || event.pointerId !== pointerId) return;
      x = event.clientX;
      y = event.clientY;
      if (state.phase === "pending") {
        if (pressMoved(state, x, y).phase === "idle") finish(false);
        return;
      }
      // Self-heal a missed pointerup/-cancel (a native drag stealing the
      // pointer): never track a bare mouse move — see ZoneDivider.
      if (event.buttons === 0) {
        finish(false);
        return;
      }
      if (frame === 0) frame = requestAnimationFrame(update);
    };

    const onPointerUp = (event: PointerEvent): void => {
      if (state.phase === "idle" || event.pointerId !== pointerId) return;
      const dragging = state.phase === "dragging";
      if (dragging) {
        // Commit the release position even when its move has not painted yet.
        // Cancel the queued frame before update replaces its handle; finish
        // then clears any auto-scroll frame that this final update schedules.
        x = event.clientX;
        y = event.clientY;
        if (frame !== 0) cancelAnimationFrame(frame);
        update();
        suppressNextClick();
      }
      finish(dragging);
    };

    const onPointerCancel = (event: PointerEvent): void => {
      if (event.pointerId === pointerId) finish(false);
    };

    // Fires whatever ends the capture, including a native drag stealing the
    // pointer without a pointerup — the one reliable "drag is over" signal.
    const onLostPointerCapture = (): void => {
      finish(false);
    };

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== "Escape" || state.phase === "idle") return;
      // Capture phase: the solo overlay row closes on Escape too, and an
      // aborted drag must not also tear down the row it happened in.
      event.preventDefault();
      event.stopPropagation();
      finish(false);
    };

    // WebKit turns a press-and-move on tile content (icons, links in plugin
    // markup) into a native HTML drag that swallows the pointer stream — the
    // failure ZoneDivider avoids by preventing its pointerdown. A long press
    // cannot do that (preventing pointerdown would also cost the tile its
    // click), so the native drag is refused at its source instead. The event
    // is composed, so it also arrives from plugin shadow roots.
    const onDragStart = (event: DragEvent): void => {
      event.preventDefault();
    };

    zone.addEventListener("pointerdown", onPointerDown);
    zone.addEventListener("lostpointercapture", onLostPointerCapture);
    zone.addEventListener("dragstart", onDragStart);
    document.addEventListener("pointermove", onPointerMove, { passive: true });
    document.addEventListener("pointerup", onPointerUp, { passive: true });
    document.addEventListener("pointercancel", onPointerCancel, {
      passive: true,
    });
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      finish(false);
      zone.removeEventListener("pointerdown", onPointerDown);
      zone.removeEventListener("lostpointercapture", onLostPointerCapture);
      zone.removeEventListener("dragstart", onDragStart);
      document.removeEventListener("pointermove", onPointerMove);
      document.removeEventListener("pointerup", onPointerUp);
      document.removeEventListener("pointercancel", onPointerCancel);
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, [ref, reserve]);
}
