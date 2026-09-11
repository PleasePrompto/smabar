import { expect, test } from "vitest";

import {
  autoScrollDelta,
  insertionIndex,
  liftStep,
  PRESS_IDLE,
  pressExceeded,
  pressHeld,
  pressMoved,
  reorderTarget,
  shiftOffset,
  slotMarker,
  slotStart,
  type ItemSpan,
} from "./dragReorder";

/** Three 40px tiles with a 6px gap, as the dock lays them out. */
const SPANS: ItemSpan[] = [
  { start: 0, end: 40 },
  { start: 46, end: 86 },
  { start: 92, end: 132 },
];

test("the drop slot follows the item centers, not the item edges", () => {
  expect(insertionIndex(SPANS, -20)).toBe(0);
  expect(insertionIndex(SPANS, 19)).toBe(0);
  // Past the first center the item would land behind it.
  expect(insertionIndex(SPANS, 21)).toBe(1);
  expect(insertionIndex(SPANS, 65)).toBe(1);
  expect(insertionIndex(SPANS, 67)).toBe(2);
  expect(insertionIndex(SPANS, 999)).toBe(3);
  expect(insertionIndex([], 10)).toBe(0);
});

test("a boundary right of the dragged item shifts by one", () => {
  // Dragging item 0 into the last gap lands it at index 2, not 3.
  expect(reorderTarget(0, 3)).toBe(2);
  expect(reorderTarget(0, 1)).toBe(0);
  // Boundaries left of the item keep their index.
  expect(reorderTarget(2, 0)).toBe(0);
  expect(reorderTarget(2, 2)).toBe(2);
});

test("auto-scroll ramps up inside the edge bands only", () => {
  // 100px wide zone, 20px bands, 10px/frame at the very edge.
  expect(autoScrollDelta(50, 0, 100, 20, 10)).toBe(0);
  expect(autoScrollDelta(0, 0, 100, 20, 10)).toBe(-10);
  expect(autoScrollDelta(10, 0, 100, 20, 10)).toBe(-5);
  expect(autoScrollDelta(100, 0, 100, 20, 10)).toBe(10);
  expect(autoScrollDelta(90, 0, 100, 20, 10)).toBe(5);
  // Dragged far past the edge: still capped at the maximum step.
  expect(autoScrollDelta(-400, 0, 100, 20, 10)).toBe(-10);
  expect(autoScrollDelta(400, 0, 100, 20, 10)).toBe(10);
  // Degenerate geometry never scrolls.
  expect(autoScrollDelta(50, 100, 100, 20, 10)).toBe(0);
});

test("moving during the hold cancels the press, holding starts the drag", () => {
  const pending = { phase: "pending", index: 2, x: 100, y: 50 } as const;

  expect(pressMoved(pending, 103, 52, 6)).toBe(pending);
  expect(pressMoved(pending, 120, 50, 6)).toEqual(PRESS_IDLE);
  expect(pressMoved(pending, 100, 60, 6)).toEqual(PRESS_IDLE);
  // Diagonal jitter counts as one distance, not per axis.
  expect(pressMoved(pending, 105, 55, 6)).toEqual(PRESS_IDLE);

  expect(pressHeld(pending)).toEqual({ phase: "dragging", index: 2 });
  // Only a pending press can become a drag.
  expect(pressHeld(PRESS_IDLE)).toBe(PRESS_IDLE);
  const dragging = { phase: "dragging", index: 1 } as const;
  expect(pressMoved(dragging, 400, 400, 6)).toBe(dragging);
  expect(pressHeld(dragging)).toBe(dragging);
});

/**
 * The same three items as a VERTICAL list: 34px rows, 4px gap, as the
 * settings panel lays them out.
 *
 * These cases exist so axis-neutrality is a property this file checks, not a
 * claim the module's doc comment makes. Every function below is fed positions
 * along the Y axis and must behave exactly as it does along X.
 */
const ROWS: ItemSpan[] = [
  { start: 0, end: 34 },
  { start: 38, end: 72 },
  { start: 76, end: 110 },
];

test("the same math places a drop slot in a vertical list", () => {
  expect(insertionIndex(ROWS, -10)).toBe(0);
  expect(insertionIndex(ROWS, 16)).toBe(0);
  // Past the first row's center the item lands below it.
  expect(insertionIndex(ROWS, 18)).toBe(1);
  expect(insertionIndex(ROWS, 54)).toBe(1);
  expect(insertionIndex(ROWS, 56)).toBe(2);
  expect(insertionIndex(ROWS, 999)).toBe(3);
});

test("auto-scroll reads the same edge band on either axis", () => {
  // Top edge of a 500px-tall scroller pulls up, bottom pushes down.
  expect(autoScrollDelta(4, 0, 500)).toBeLessThan(0);
  expect(autoScrollDelta(496, 0, 500)).toBeGreaterThan(0);
  expect(autoScrollDelta(250, 0, 500)).toBe(0);
});

test("the press tolerance is a circle, so a diagonal nudge counts", () => {
  const down = { x: 100, y: 100 };
  expect(pressExceeded(down, 104, 100)).toBe(false);
  expect(pressExceeded(down, 107, 100)).toBe(true);
  // 5px on each axis is under the tolerance per axis but 7.07px of travel —
  // the case a per-axis check would wave through.
  expect(pressExceeded(down, 105, 105)).toBe(true);
});

/**
 * Four rows of DELIBERATELY different heights (20/40/20/30) with the same 4px
 * gap. The live reflow claims to be independent of every item's size but the
 * dragged one; a uniform fixture could not tell a correct implementation from
 * one that happens to work when everything is the same size.
 */
const MIXED: ItemSpan[] = [
  { start: 0, end: 20 },
  { start: 24, end: 64 },
  { start: 68, end: 88 },
  { start: 92, end: 122 },
];

test("one seam describes the whole stack, whatever the rows measure", () => {
  // Own height + the single gap that closes with it.
  expect(liftStep(MIXED, 0)).toBe(24);
  expect(liftStep(MIXED, 1)).toBe(44);
  expect(liftStep(MIXED, 2)).toBe(24);
  expect(liftStep(MIXED, 3)).toBe(34);
  // Nothing to lift, and nothing to derive a gap from.
  expect(liftStep([], 0)).toBe(0);
  expect(liftStep([{ start: 0, end: 20 }], 0)).toBe(20);
  expect(liftStep(MIXED, 9)).toBe(0);
});

test("a lifted item moves every neighbour it passes by exactly one slot", () => {
  // Travelling down: everything between the home and the boundary moves up.
  expect(shiftOffset(1, 0, 3, 38)).toBe(-38);
  expect(shiftOffset(2, 0, 3, 38)).toBe(-38);
  expect(shiftOffset(3, 0, 3, 38)).toBe(0);
  // Travelling up: everything from the boundary to the home moves down.
  expect(shiftOffset(0, 3, 0, 38)).toBe(38);
  expect(shiftOffset(2, 3, 0, 38)).toBe(38);
  // The lifted item is never shifted — the hook translates it separately.
  expect(shiftOffset(0, 0, 3, 38)).toBe(0);
  expect(shiftOffset(3, 3, 0, 38)).toBe(0);
  // An untouched neighbour outside the travelled range.
  expect(shiftOffset(0, 1, 3, 38)).toBe(0);
  // The two boundaries that mean "stay": nothing moves, which has to agree
  // with reorderTarget resolving both of them to the item's own index.
  for (const insertAt of [1, 2]) {
    for (const index of [0, 1, 2, 3]) {
      expect(
        shiftOffset(index, 1, insertAt, 38),
        `${String(index)}@${String(insertAt)}`,
      ).toBe(0);
    }
    expect(reorderTarget(1, insertAt)).toBe(1);
  }
});

test("the hole opens where the neighbours have just made room", () => {
  // Row 0 dragged to the very end: row 2 moved up, so the hole starts one
  // own-height short of where row 2 used to end.
  expect(slotStart(MIXED, 0, 4)).toBe(102);
  // Row 3 dragged to the very top: the hole is exactly row 0's old start.
  expect(slotStart(MIXED, 3, 0)).toBe(0);
  // Both boundaries that mean "stay" resolve to the item's own place.
  expect(slotStart(MIXED, 1, 1)).toBe(24);
  expect(slotStart(MIXED, 1, 2)).toBe(24);
  // One step up.
  expect(slotStart(MIXED, 2, 1)).toBe(24);
  expect(slotStart([], 0, 0)).toBe(0);
});

test("a position exactly on an item's center lands AFTER it", () => {
  // insertionIndex breaks on a strict `pos < mid`, so a center is a tie and
  // the tie goes to the slot below. That single rule cuts both ways: it is
  // what makes the LAST boundary reachable from a position clamped to the
  // last item's home, and it is what made the FIRST boundary unreachable
  // while useListReorder read the slot from a clamped position. The fix
  // belongs in the caller (read the slot unclamped) — flipping this to `<=`
  // would only move the dead end from one end of the list to the other.
  expect(insertionIndex(ROWS, 17)).toBe(1);
  expect(insertionIndex(ROWS, 93)).toBe(3);
});

test("the insertion line is centred in the seam the shift just opened", () => {
  // Row 0 (20px) heading for boundary 2: row 1 has moved up a whole step, so
  // the hole is [44, 64] and the seam above it is [40, 44] — a 2px line
  // centred in it starts at 41 — a whole row away from the 66 the untouched
  // layout's own seam sits at.
  expect(slotMarker(MIXED, 0, 2, 2)).toBe(41);
  expect(slotMarker(MIXED, 3, 2, 2)).toBe(65);
  // Both extremes clamp flush into the band instead of hanging half outside,
  // which is what boundaryPosition did for the outer edges.
  expect(slotMarker(MIXED, 3, 0, 2)).toBe(0);
  expect(slotMarker(MIXED, 0, 4, 2)).toBe(99);
  expect(slotMarker([], 0, 0, 2)).toBe(0);
});
