import { expect, test } from "vitest";

import {
  autohideRevealed,
  pointerInAutohideRegion,
  pushPointerSample,
} from "./useAutohide";

const HOTZONE = { left: 268, top: 892, right: 1132, bottom: 900 };
const SURFACE = { left: 268, top: 828, right: 1132, bottom: 884 };

test("autohide visibility rule honors pointer and interaction surfaces", () => {
  expect(autohideRevealed("reserve", false, false)).toBe(true);
  expect(autohideRevealed("autohide", false, false)).toBe(false);
  expect(autohideRevealed("autohide", true, false)).toBe(true);
  expect(autohideRevealed("autohide", false, true)).toBe(true);
});

test("input regions stay bounded by the native dock window", () => {
  const regions = [HOTZONE, SURFACE];
  expect(pointerInAutohideRegion(700, 897, regions)).toBe(true);
  // Same edge, but beside the centered dock — a full-width strip would pull
  // the bar out from anywhere along the screen edge.
  expect(pointerInAutohideRegion(60, 897, regions)).toBe(false);
  expect(pointerInAutohideRegion(700, 850, regions)).toBe(true);
});

test("revealed bar keeps the pointer inside across the edge gap", () => {
  // The outer strip always bridges the edge gap. Native window movement
  // carries these stable local regions into and out of the screen.
  const revealed = [{ ...HOTZONE, top: 884 }, SURFACE];
  expect(pointerInAutohideRegion(700, 888, revealed)).toBe(true);
  expect(pointerInAutohideRegion(700, 856, revealed)).toBe(true);
  // The magnify headroom above the row belongs to no input region: the
  // pointer there is outside the bar as far as the core is concerned.
  expect(pointerInAutohideRegion(700, 824, revealed)).toBe(false);
  expect(pointerInAutohideRegion(1200, 856, revealed)).toBe(false);
});

test("a dropped pointer sample never matches a region", () => {
  // The pointer left the window: no coordinates are known any more, so the
  // grace period must not re-validate itself back into "inside".
  expect(
    pointerInAutohideRegion(Number.NaN, Number.NaN, [HOTZONE, SURFACE]),
  ).toBe(false);
});

test("a drag over the bar holds it out like an open surface", () => {
  // The hook ORs a running tile reorder and an OS file drag into
  // `surfaceOpen`: both hold the pointer (no mousemove reaches the webview),
  // so the coordinate rule alone would retract the bar mid-interaction.
  expect(autohideRevealed("autohide", false, true)).toBe(true);
  // Nothing dragging and no pointer: the bar retracts as before.
  expect(autohideRevealed("autohide", false, false)).toBe(false);
});

test("a file drag reveals the bar from the same edge strip as the mouse", () => {
  // While hidden, the hot strip is the only region still on screen, so
  // it is the only place an XDND drag can reach the window at all. A
  // drag position landing there must read exactly like a mouse position:
  // that reveal is what puts the drop target under the pointer.
  const hidden = [HOTZONE, null];
  expect(pointerInAutohideRegion(700, 897, hidden)).toBe(true);
  expect(pointerInAutohideRegion(700, 700, hidden)).toBe(false);
  // Physical drag coordinates are converted before they get here — a raw
  // 2x-scaled sample would miss the strip entirely.
  expect(pointerInAutohideRegion(1400, 1794, hidden)).toBe(false);
});

test("pushing a sample without a mounted autohide bar is a no-op", () => {
  // float/reserve mount no listener; a file drag must not throw there.
  expect(() => {
    pushPointerSample(10, 10);
  }).not.toThrow();
});
