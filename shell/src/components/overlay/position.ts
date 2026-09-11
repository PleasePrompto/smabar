/**
 * Placement math shared by context menus and tooltips. Pure functions: the surfaces measure themselves
 * and hand the numbers in, so every edge case is unit-testable without a
 * layout engine (same pattern as `flyoutDirection` / `autohideRevealed`).
 */

export interface OverlaySize {
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface Placement {
  left: number;
  top: number;
}

/** Keep-out distance from every window edge. */
export const OVERLAY_MARGIN_PX = 8;
/** How far a submenu overlaps its parent menu, hiding the seam. */
export const SUBMENU_OVERLAP_PX = 4;
/**
 * Keeps a surface of `size` inside `[margin, extent - margin]`. A surface
 * larger than the window pins to the leading margin instead of jumping to a
 * negative offset.
 */
export function clampAxis(
  start: number,
  size: number,
  extent: number,
  margin: number = OVERLAY_MARGIN_PX,
): number {
  const max = extent - margin - size;
  if (max <= margin) return margin;
  return Math.min(Math.max(start, margin), max);
}

/**
 * Places a surface after `at` (down / to the right) and FLIPS it before the
 * point when that side has no room but the other one does — the desktop
 * convention: a menu opened at the bottom screen edge opens upwards.
 */
function flipAxis(
  at: number,
  size: number,
  extent: number,
  margin: number,
): number {
  if (at + size <= extent - margin) return at;
  return at - size >= margin ? at - size : at;
}

/**
 * Context-menu placement: down-right from the pointer, flipped at the far
 * edges, then clamped into the window.
 */
export function anchorMenu(
  point: Point,
  size: OverlaySize,
  viewport: Viewport,
  margin: number = OVERLAY_MARGIN_PX,
): Placement {
  return {
    left: clampAxis(
      flipAxis(point.x, size.width, viewport.width, margin),
      size.width,
      viewport.width,
      margin,
    ),
    top: clampAxis(
      flipAxis(point.y, size.height, viewport.height, margin),
      size.height,
      viewport.height,
      margin,
    ),
  };
}

/** Geometry a submenu opens against: its parent menu box and item row. */
export interface SubmenuAnchor {
  menuLeft: number;
  menuRight: number;
  itemTop: number;
}

/**
 * Submenu placement: to the RIGHT of the parent menu, top-aligned with the
 * owning row. Without room on the right the submenu flips to the parent's
 * left side (never on top of it); both axes are clamped afterwards.
 */
export function anchorSubmenu(
  anchor: SubmenuAnchor,
  size: OverlaySize,
  viewport: Viewport,
  margin: number = OVERLAY_MARGIN_PX,
): Placement {
  const right = anchor.menuRight - SUBMENU_OVERLAP_PX;
  const left =
    right + size.width <= viewport.width - margin
      ? right
      : anchor.menuLeft - size.width + SUBMENU_OVERLAP_PX;
  return {
    left: clampAxis(left, size.width, viewport.width, margin),
    top: clampAxis(anchor.itemTop, size.height, viewport.height, margin),
  };
}
