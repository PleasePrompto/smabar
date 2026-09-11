import { expect, test } from "vitest";

import {
  anchorMenu,
  anchorSubmenu,
  clampAxis,
  OVERLAY_MARGIN_PX,
  SUBMENU_OVERLAP_PX,
} from "./position";

const VIEWPORT = { width: 1400, height: 900 };

test("clampAxis keeps a surface inside the margins", () => {
  expect(clampAxis(100, 200, 1000, 8)).toBe(100);
  expect(clampAxis(-40, 200, 1000, 8)).toBe(8);
  expect(clampAxis(900, 200, 1000, 8)).toBe(792);
  // A surface larger than the window pins to the leading margin instead of
  // jumping to a negative offset.
  expect(clampAxis(50, 2000, 1000, 8)).toBe(8);
});

test("the menu opens down-right where there is room", () => {
  expect(
    anchorMenu({ x: 300, y: 200 }, { width: 200, height: 240 }, VIEWPORT),
  ).toEqual({ left: 300, top: 200 });
});

test("the menu flips at the right and bottom edges", () => {
  const size = { width: 200, height: 240 };
  // Bottom edge: the dock case — the menu must grow upwards, not off-screen.
  expect(anchorMenu({ x: 300, y: 880 }, size, VIEWPORT).top).toBe(640);
  expect(anchorMenu({ x: 1380, y: 200 }, size, VIEWPORT).left).toBe(1180);
  const corner = anchorMenu({ x: 1380, y: 880 }, size, VIEWPORT);
  expect(corner).toEqual({ left: 1180, top: 640 });
});

test("a menu that fits on neither side is clamped, never flipped off-screen", () => {
  // Too close to the left edge to flip AND too wide to fit after the point.
  const place = anchorMenu(
    { x: 40, y: 40 },
    { width: 1390, height: 200 },
    VIEWPORT,
  );
  expect(place.left).toBe(OVERLAY_MARGIN_PX);
});

test("a submenu opens beside its parent and flips when the right is full", () => {
  const anchor = {
    kind: "submenu" as const,
    menuLeft: 300,
    menuRight: 500,
    itemTop: 400,
  };
  const size = { width: 180, height: 120 };
  expect(anchorSubmenu(anchor, size, VIEWPORT)).toEqual({
    left: 500 - SUBMENU_OVERLAP_PX,
    top: 400,
  });

  const atRightEdge = { ...anchor, menuLeft: 1200, menuRight: 1392 };
  expect(anchorSubmenu(atRightEdge, size, VIEWPORT).left).toBe(
    1200 - 180 + SUBMENU_OVERLAP_PX,
  );
});
