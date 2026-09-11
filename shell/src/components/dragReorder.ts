/**
 * Pure drag-reorder math and press state machine (vitest-covered, no DOM).
 *
 * Axis-neutral by construction: every function takes plain scalars along the
 * drag axis, so the bar's horizontal zones and the settings panel's vertical
 * lists share it. `dragReorder.test.ts` proves that with both a horizontal and
 * a vertical fixture — it is a checked property, not a claim in this comment.
 *
 * The `liftStep`/`shiftOffset`/`slotStart` trio at the bottom describes the
 * LIVE reflow: where every neighbour stands while an item is out of the stack,
 * and where the hole it leaves behind sits. `reorderShift.ts` writes those
 * numbers to the DOM; both drag hooks read them.
 */

/**
 * Hold time that arms a drag. dnd-kit's touch sensor uses 250 ms; a dock
 * tile is also a click target (it launches an app), so a slightly longer
 * hold keeps "press to launch" and "press to move" unambiguous.
 */
export const LONG_PRESS_MS = 300;

/**
 * Motion during the hold that cancels it — the press then stays an ordinary
 * click. dnd-kit tolerates 5 px for touch; 6 px also absorbs the hand jitter
 * of a mouse user without swallowing an intentional swipe.
 */
export const PRESS_TOLERANCE_PX = 6;

/** Edge band of an overflowing zone that auto-scrolls during a drag. */
export const AUTOSCROLL_EDGE_PX = 48;

/** Fastest auto-scroll step in px per frame, reached at the very edge. */
export const AUTOSCROLL_MAX_PX = 14;

/** Extent of one item ALONG THE DRAG AXIS, in the scroll content's space. */
export interface ItemSpan {
  start: number;
  end: number;
}

/**
 * Gap the dragged item would drop into: the number of items whose center
 * lies before `pos`. Result range is 0…spans.length (a boundary index, not
 * an item index), so it also names the slot after the last item.
 */
export function insertionIndex(
  spans: readonly ItemSpan[],
  pos: number,
): number {
  let index = 0;
  for (const span of spans) {
    if (pos < (span.start + span.end) / 2) break;
    index += 1;
  }
  return index;
}

/**
 * Array index the dragged item ends up at. `insertAt` is a boundary index in
 * the ORIGINAL array, so every boundary right of the item shifts by one once
 * the item is lifted out.
 */
export function reorderTarget(from: number, insertAt: number): number {
  return insertAt > from ? insertAt - 1 : insertAt;
}

/**
 * Auto-scroll step for a pointer near the edge of a scrolling zone: zero in
 * the middle, ramping linearly to `max` at the very edge. Negative scrolls
 * left. A zone narrower than two edge bands still behaves (the ramps just
 * overlap, and the nearer edge wins because the left branch is tested first).
 */
export function autoScrollDelta(
  x: number,
  left: number,
  right: number,
  edge = AUTOSCROLL_EDGE_PX,
  max = AUTOSCROLL_MAX_PX,
): number {
  if (edge <= 0 || right <= left) return 0;
  if (x < left + edge) {
    return -Math.ceil((Math.min(edge, left + edge - x) / edge) * max);
  }
  if (x > right - edge) {
    return Math.ceil((Math.min(edge, x - right + edge) / edge) * max);
  }
  return 0;
}

/**
 * Press lifecycle of one pointer on a zone: `pending` while the hold timer
 * runs (the click is still the outcome), `dragging` once it fired.
 */
export type PressState =
  | { phase: "idle" }
  | { phase: "pending"; index: number; x: number; y: number }
  | { phase: "dragging"; index: number };

export const PRESS_IDLE: PressState = { phase: "idle" };

/**
 * Whether a pointer has left the tolerance circle around where it went down.
 *
 * A circle, not a per-axis check: a diagonal nudge of 5 px on each axis is
 * 7 px of travel and should count, which an axis-wise comparison would miss.
 * The two gestures read it in opposite directions — the bar CANCELS a pending
 * press with it, the settings list STARTS a drag with it.
 */
export function pressExceeded(
  from: { x: number; y: number },
  x: number,
  y: number,
  tolerance = PRESS_TOLERANCE_PX,
): boolean {
  return Math.hypot(x - from.x, y - from.y) > tolerance;
}

/**
 * A moving pointer cancels a pending press once it leaves the tolerance
 * circle — the tile then behaves like an untouched click target. A drag in
 * progress and an idle zone are unaffected.
 */
export function pressMoved(
  state: PressState,
  x: number,
  y: number,
  tolerance = PRESS_TOLERANCE_PX,
): PressState {
  if (state.phase !== "pending") return state;
  return pressExceeded(state, x, y, tolerance) ? PRESS_IDLE : state;
}

/** The hold timer fired: a pending press becomes a drag, nothing else moves. */
export function pressHeld(state: PressState): PressState {
  return state.phase === "pending"
    ? { phase: "dragging", index: state.index }
    : state;
}

/**
 * How far the stack closes up while item `index` is lifted out of it: the
 * item's own extent plus the ONE gap that closes with it.
 *
 * The gap is measured from the first seam rather than passed in, because it
 * is a theme token (`--sb-space-2xs` in the settings list, `--sb-tile-gap` in
 * a bar zone) that `--sb-scale` rescales — a constant here would drift.
 *
 * One seam describes them all, and that holds for items of DIFFERING extent:
 * in a flex container `start(i) = Σ_{j<i} size_j + gap·i`, so removing an
 * item above `i` drops the sum by that item's size and the count by one. The
 * shift is therefore independent of every other item's size. The single
 * assumption is a uniform gap, which `gap` in both containers guarantees.
 */
export function liftStep(spans: readonly ItemSpan[], index: number): number {
  const own = spans[index];
  if (own === undefined) return 0;
  const first = spans[0];
  const second = spans[1];
  const gap =
    first === undefined || second === undefined ? 0 : second.start - first.end;
  return own.end - own.start + gap;
}

/**
 * How far item `index` steps aside while item `from` travels to boundary
 * `insertAt`: exactly one step for everything the lifted item passes, zero
 * for everything else — the lifted item itself included, and both boundaries
 * that mean "stay put" (`from` and `from + 1`) yield an all-zero row.
 */
export function shiftOffset(
  index: number,
  from: number,
  insertAt: number,
  step: number,
): number {
  if (index > from && index < insertAt) return -step;
  if (index >= insertAt && index < from) return step;
  return 0;
}

/**
 * Where the hole opens, in the coordinate space measured BEFORE the drag:
 * the leading edge the dragged item's slot has once every neighbour has
 * stepped aside.
 *
 * At or before its home the hole is exactly where the item now standing at
 * `insertAt` used to start; past it, the last item it overtook has moved up
 * by a whole step, so the hole begins one own-extent short of that item's
 * original trailing edge. The two branches are why a marker cannot simply
 * reuse `boundaryPosition` once the neighbours actually move.
 */
export function slotStart(
  spans: readonly ItemSpan[],
  from: number,
  insertAt: number,
): number {
  const own = spans[from];
  if (own === undefined) return 0;
  if (insertAt <= from) return spans[insertAt]?.start ?? own.start;
  return (spans[insertAt - 1]?.end ?? own.end) - (own.end - own.start);
}

/**
 * Where the insertion line goes once the neighbours have moved: centred in
 * the seam that just opened, and never outside the items' own band.
 *
 * Reading the seam out of the ORIGINAL layout would be exactly one step
 * wrong as soon as the neighbours move, which is why this composes with
 * {@link slotStart} rather than measuring between two untouched items. The
 * clamp keeps it inside the band at both ends, where the seam is half
 * outside it.
 */
export function slotMarker(
  spans: readonly ItemSpan[],
  from: number,
  insertAt: number,
  thickness: number,
): number {
  const first = spans[0];
  const last = spans[spans.length - 1];
  const own = spans[from];
  if (first === undefined || last === undefined || own === undefined) return 0;
  const gap = liftStep(spans, from) - (own.end - own.start);
  const centre = slotStart(spans, from, insertAt) - gap / 2;
  return Math.max(
    first.start,
    Math.min(centre - thickness / 2, last.end - thickness),
  );
}
