/**
 * Drag-to-reorder for a VERTICAL list in the settings panel.
 *
 * The bar has its own hook (`components/bar/useDragReorder.ts`) and this is
 * deliberately not it. They share the parts that are hard and worth sharing —
 * the geometry and press math in `components/dragReorder.ts` and the live
 * reflow in `components/reorderShift.ts`, both axis-neutral and unit-tested —
 * and nothing else, because the bar's half is built around concerns a
 * settings row does not have: the fisheye magnify wipe, horizontal zone
 * scrolling, a 300 ms long-press (a bar tile launches an app on click, a row
 * does not) and the click suppression that follows from it. Carrying those
 * here as branches would cost more than this file.
 *
 * It still reports `setReordering(true)` so the shell can suppress conflicting
 * gestures while the native settings window owns the pointer capture.
 */
import { useEffect, useRef } from "react";

import {
  AUTOSCROLL_MAX_PX,
  autoScrollDelta,
  insertionIndex,
  pressExceeded,
  reorderTarget,
  slotMarker,
  type ItemSpan,
} from "../dragReorder";
import { applyShift, clearShift, setShift } from "../reorderShift";
import { DRAG_MARKER } from "../../styles/layers";
import { useSmabar } from "../../store/bar";

/** Thickness of the insertion line. The hook owns it; the CSS must not. */
const MARKER_THICKNESS_PX = 2;

/** Puts the insertion line `offset` into the list, in viewport space. */
function place(line: HTMLElement, rect: DOMRect, offset: number): void {
  line.style.setProperty(
    "translate",
    `${rect.left.toFixed(2)}px ${(rect.top + offset).toFixed(2)}px`,
  );
}

interface ListReorderOptions {
  /** How many rows are reorderable. They lead the list, contiguously. */
  count: number;
  onReorder: (from: number, to: number) => void;
}

interface DragState {
  pointerId: number;
  index: number;
  /** Where the pointer went down, for the movement threshold. */
  x: number;
  y: number;
  started: boolean;
  spans: ItemSpan[];
  /**
   * The reorderable rows, captured at drag start. Cached rather than
   * re-queried because clean-up must reach exactly the nodes that were
   * written to, even if React re-rendered the list underneath the gesture —
   * a leftover `translate` would corrupt the NEXT drag's measurements.
   */
  items: HTMLElement[];
  source: HTMLElement | null;
  grabOffset: number;
  scroller: HTMLElement | null;
  scrollBehavior: string;
  insertAt: number;
}

export function useListReorder(
  ref: React.RefObject<HTMLElement | null>,
  options: ListReorderOptions,
): void {
  // Read through a ref so the listener effect does not re-bind on every
  // render — the list rebuilds its rows constantly, and re-binding mid-drag
  // would drop the gesture.
  const latest = useRef(options);
  useEffect(() => {
    latest.current = options;
  });

  useEffect(() => {
    const list = ref.current;
    if (list === null) return;

    let drag: DragState | null = null;
    let marker: HTMLElement | null = null;
    let frame = 0;

    /** The reorderable rows, in DOM order. Never `children` — the dimmed
     * tail rows are children too and would shift every index. */
    const rows = (): HTMLElement[] => [
      ...list.querySelectorAll<HTMLElement>(":scope > [data-reorder-index]"),
    ];

    const cleanUp = () => {
      if (frame !== 0) cancelAnimationFrame(frame);
      frame = 0;
      marker?.remove();
      marker = null;
      if (drag !== null) {
        // Attribute first, styles second: it is what arms the translate
        // transition, so clearing the shift while it is still set would
        // animate every row back to where it came from — a slide the eye
        // reads as the drop being undone, right as the new order arrives.
        if (drag.started) {
          list.removeAttribute("data-reordering");
          useSmabar.getState().setReordering(false);
        }
        drag.source?.classList.remove("settings-drag-source");
        clearShift(drag.items);
        if (drag.scroller !== null) {
          drag.scroller.style.scrollBehavior = drag.scrollBehavior;
        }
      }
      drag = null;
    };

    const begin = (state: DragState) => {
      const items = rows();
      // The guard is true by construction thanks to the attribute selector,
      // so a mismatch means the row model and the DOM disagree — bail loudly
      // rather than write a corrupted order.
      if (items.length !== latest.current.count) return false;
      const listRect = list.getBoundingClientRect();
      state.spans = items.map((row) => {
        const rect = row.getBoundingClientRect();
        return {
          start: rect.top - listRect.top,
          end: rect.bottom - listRect.top,
        };
      });
      const own = state.spans[state.index];
      if (own === undefined) return false;
      state.items = items;
      state.grabOffset = state.y - listRect.top - own.start;
      state.source = items[state.index] ?? null;
      state.source?.classList.add("settings-drag-source");
      // The panel's scroller animates every scrollTop write (.sb-scroll sets
      // scroll-behavior: smooth), which turns a per-frame nudge into a stutter.
      const scroller = list.closest<HTMLElement>(".sb-scroll");
      state.scroller = scroller;
      if (scroller !== null) {
        state.scrollBehavior = scroller.style.scrollBehavior;
        scroller.style.scrollBehavior = "auto";
      }
      marker = document.createElement("div");
      marker.className = "settings-drop-marker";
      marker.setAttribute("aria-hidden", "true");
      // The number belongs to layers.ts, not to the stylesheet: the panel is
      // portalled to <body> at PANEL (100), so a marker parked below that
      // paints BEHIND the opaque settings window and is simply never seen.
      marker.style.zIndex = DRAG_MARKER;
      marker.style.height = `${String(MARKER_THICKNESS_PX)}px`;
      marker.style.width = `${String(listRect.width)}px`;
      // Placed before it enters the document: a transition never runs on an
      // element's initial style, so the line appears where it belongs instead
      // of flying in from the viewport corner.
      place(
        marker,
        listRect,
        slotMarker(state.spans, state.index, state.index, MARKER_THICKNESS_PX),
      );
      document.body.append(marker);
      list.setAttribute("data-reordering", "");
      useSmabar.getState().setReordering(true);
      state.started = true;
      return true;
    };

    const update = (y: number) => {
      if (drag === null || marker === null) return;
      const listRect = list.getBoundingClientRect();
      const scroller = drag.scroller;
      if (
        scroller !== null &&
        scroller.scrollHeight - scroller.clientHeight > 1
      ) {
        const rect = scroller.getBoundingClientRect();
        const step = autoScrollDelta(y, rect.top, rect.bottom);
        if (step !== 0) {
          scroller.scrollTop += Math.max(
            -AUTOSCROLL_MAX_PX,
            Math.min(AUTOSCROLL_MAX_PX, step),
          );
        }
      }
      const own = drag.spans[drag.index];
      if (own === undefined) return;
      const height = own.end - own.start;
      const first = drag.spans[0];
      const last = drag.spans[drag.spans.length - 1];
      if (first === undefined || last === undefined) return;
      const wanted = y - listRect.top - drag.grabOffset;
      // The clamp is a PAINTING limit, never an intent one: the scroller also
      // holds headings and the per-plugin forms, and a row wandering over a
      // heading reads as broken. The slot below is therefore read from the
      // UNCLAMPED position — clamped, the row's centre lands exactly on the
      // first row's centre, insertionIndex's tie goes to the row below, and
      // the top slot was unreachable however far up you dragged.
      const top = Math.max(first.start, Math.min(wanted, last.end - height));
      if (drag.source !== null) setShift(drag.source, top - own.start, "y");
      const next = insertionIndex(drag.spans, wanted + height / 2);
      // The rows only need writing when the slot actually changes; the line
      // needs it every frame because it lives in viewport space and the
      // scroller may be moving the list out from under it.
      if (next !== drag.insertAt) {
        drag.insertAt = next;
        applyShift(drag.items, drag.spans, drag.index, next, "y");
      }
      place(
        marker,
        listRect,
        slotMarker(drag.spans, drag.index, next, MARKER_THICKNESS_PX),
      );
    };

    const onPointerDown = (event: PointerEvent) => {
      if (event.button !== 0 || drag !== null) return;
      const target = event.target;
      if (!(target instanceof Element)) return;
      const handle = target.closest<HTMLElement>("[data-drag-handle]");
      if (handle === null) return;
      const row = handle.closest<HTMLElement>("[data-reorder-index]");
      if (row === null) return;
      const index = rows().indexOf(row);
      if (index < 0) return;
      // A handle has no click of its own, so preventing the default here is
      // free — and it buys three things at once: no text selection, no native
      // WebKit drag, and the focus does NOT move into the row, so a mouse
      // drag cannot steal a keyboard user's tab position.
      event.preventDefault();
      drag = {
        pointerId: event.pointerId,
        index,
        x: event.clientX,
        y: event.clientY,
        started: false,
        spans: [],
        items: [],
        source: null,
        grabOffset: 0,
        scroller: null,
        scrollBehavior: "",
        insertAt: index,
      };
      list.setPointerCapture(event.pointerId);
    };

    const onPointerMove = (event: PointerEvent) => {
      if (drag?.pointerId !== event.pointerId) return;
      // The button vanished without an up event (a lost release outside the
      // window); recover rather than track a phantom drag.
      if (event.buttons === 0) {
        finish(false);
        return;
      }
      if (!drag.started) {
        if (!pressExceeded(drag, event.clientX, event.clientY)) return;
        if (!begin(drag)) {
          drag = null;
          return;
        }
      }
      const y = event.clientY;
      if (frame !== 0) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        update(y);
      });
    };

    const finish = (commit: boolean) => {
      if (drag === null) return;
      const { started, index, insertAt, pointerId } = drag;
      if (list.hasPointerCapture(pointerId)) {
        list.releasePointerCapture(pointerId);
      }
      cleanUp();
      if (!commit || !started) return;
      const to = reorderTarget(index, insertAt);
      if (to !== index) latest.current.onReorder(index, to);
    };

    const onPointerUp = (event: PointerEvent) => {
      if (drag?.pointerId === event.pointerId) finish(true);
    };
    const onPointerCancel = () => {
      finish(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (drag === null || event.key !== "Escape") return;
      // Capture phase and stopped: the panel's own Escape would close the
      // whole settings window mid-drag.
      event.preventDefault();
      event.stopPropagation();
      finish(false);
    };

    list.addEventListener("pointerdown", onPointerDown);
    list.addEventListener("pointermove", onPointerMove);
    list.addEventListener("pointerup", onPointerUp);
    list.addEventListener("pointercancel", onPointerCancel);
    list.addEventListener("lostpointercapture", onPointerCancel);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      list.removeEventListener("pointerdown", onPointerDown);
      list.removeEventListener("pointermove", onPointerMove);
      list.removeEventListener("pointerup", onPointerUp);
      list.removeEventListener("pointercancel", onPointerCancel);
      list.removeEventListener("lostpointercapture", onPointerCancel);
      window.removeEventListener("keydown", onKeyDown, true);
      // A drag torn down mid-gesture must not leave `reordering` set: the
      // input shape would keep a full-window rect forever and the desktop
      // behind the bar would stop taking clicks.
      cleanUp();
    };
  }, [ref]);
}
