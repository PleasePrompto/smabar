/**
 * The live reflow of a reorder drag: the items a dragged one passes step
 * aside, so the hole that opens IS the drop target. Shared by both drag
 * hooks — the bar's zones along X, the settings list along Y.
 *
 * Written as the standalone `translate` property, never `transform`, and that
 * one choice carries the whole file:
 *
 * - A bar tile's `transform` already belongs to the fisheye
 *   (`scale(var(--magnify))`). Two owners on one property means one of them
 *   has to re-state the other's value every frame; two properties means
 *   neither has to know the other exists.
 * - `ui-kit.css` gives `.sb-row` a 140 ms transition that names `transform`.
 *   A shift written there would inherit that timing silently; written as
 *   `translate` the component's own stylesheet declares the timing outright,
 *   without copying ui-kit's property list (which would rot the next time
 *   that list is edited).
 * - Outside a running drag NO rule transitions `translate`, so
 *   {@link clearShift} is instant on every exit path — commit, Escape,
 *   pointercancel, a lost capture, an unmount mid-gesture. A shift that
 *   animated its way back would race the incoming order.
 *
 * The property is Safari 14.1 / WebKitGTK 2.32 and up; smabar targets 2.52,
 * and `kit-components.css` already uses it.
 */
import { liftStep, shiftOffset, type ItemSpan } from "./dragReorder";

export type DragAxis = "x" | "y";

/**
 * Opens the hole item `from` would drop into at boundary `insertAt`.
 *
 * `items` are the REORDERABLE elements only, index-aligned with `spans`. A
 * container may hold more than those — the settings list keeps a dimmed tail
 * of switched-off plugins in the same flex column — and anything outside this
 * array is never written to, so it cannot drift.
 */
export function applyShift(
  items: readonly HTMLElement[],
  spans: readonly ItemSpan[],
  from: number,
  insertAt: number,
  axis: DragAxis,
): void {
  const step = liftStep(spans, from);
  items.forEach((item, index) => {
    if (index === from) return;
    const offset = shiftOffset(index, from, insertAt, step);
    if (offset === 0) {
      item.style.removeProperty("translate");
      return;
    }
    const px = `${offset.toFixed(2)}px`;
    item.style.setProperty("translate", axis === "x" ? px : `0 ${px}`);
  });
}

/** Moves one element along the drag axis — the dragged item itself. */
export function setShift(
  item: HTMLElement,
  offset: number,
  axis: DragAxis,
): void {
  const px = `${offset.toFixed(2)}px`;
  item.style.setProperty("translate", axis === "x" ? px : `0 ${px}`);
}

/** Puts every item back where the layout says it belongs. */
export function clearShift(items: readonly HTMLElement[]): void {
  for (const item of items) item.style.removeProperty("translate");
}
