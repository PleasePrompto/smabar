import {
  magnifyScale,
  type EffectsConfig,
  type LabelMode,
} from "../../store/bar";
import {
  clampIconSize,
  clampLabelSize,
  clampNeighbors,
  pxToRem,
} from "../settings/model";

/** Pure bar geometry + fisheye math (vitest-covered, no DOM). */

/** Display options of the shortcut zone that drive the bar geometry. */
export interface ShortcutMetrics {
  labels: LabelMode;
  iconSize: number;
  labelSize: number;
}

/** Vertical gap between icon and label in the `below` label mode, in px at
 * 100% scale (must match the tile's gap class in ShortcutZone — Tailwind's
 * `gap-1` is 0.25rem, so this is expressed in rem too and both scale). */
export const LABEL_GAP_PX = 4;

/**
 * CSS expression for the effective bar row height: with labels below the
 * icons the row grows to fit icon + label, otherwise it is the token height
 * or the icon plus padding, whichever is larger. The vertical whitespace
 * token `--sb-bar-pad-y` (6px in the bundled default theme)
 * drives the row's real block padding (BarShell); the whitespace slider also
 * scales `--sb-bar-height` so the floor follows it. With card tile chrome
 * the shortcut tiles wrap their icons in `--sb-tile-pad-y` padding
 * (bar.css), so that is budgeted too. BarShell publishes the result
 * as `--sb-bar-row-height`; a portaled surface (which cannot inherit that
 * var) recomputes it through this same function.
 */
/** What a card tile adds around its content: padding plus its own border,
 * which takes space even while a theme paints it transparent. */
const CARD_PAD =
  " + 2 * var(--sb-tile-pad-y, 0.375rem) + 2 * var(--sb-border-width, 1px)";
/** What the row adds around a tile: its block padding plus the bar frame. */
const ROW_PAD =
  " + 2 * var(--sb-bar-pad-y, 0.375rem) + 2 * var(--sb-bar-border-width, var(--sb-border-width, 1px))";

export function rowHeightExpr(
  shortcuts: ShortcutMetrics,
  cardTiles = false,
): string {
  const icon = clampIconSize(shortcuts.iconSize);
  const cardPad = cardTiles ? CARD_PAD : "";
  if (shortcuts.labels === "below") {
    const label = clampLabelSize(shortcuts.labelSize);
    return `calc(${pxToRem(icon + LABEL_GAP_PX + label)}${cardPad}${ROW_PAD})`;
  }
  return `max(var(--sb-bar-height, 3.25rem), calc(${pxToRem(icon)}${cardPad}${ROW_PAD}))`;
}

/**
 * The row height once the tile covers are known: the tallest cover plus
 * the card padding, border and row padding, or `base` (rowHeightExpr) when every cover fits
 * it. `tallestCover` is a measured CSS px value (PluginContent reports each
 * cover's scrollHeight), so it stays in px rather than the rem the
 * configured sizes use. 0 means nothing measured yet.
 */
export function coverRowHeightExpr(base: string, tallestCover: number): string {
  if (!Number.isFinite(tallestCover) || tallestCover <= 0) return base;
  return `max(${base}, calc(${String(Math.ceil(tallestCover))}px${CARD_PAD}${ROW_PAD}))`;
}

/** The tallest reported cover, 0 without any. */
export function tallestCover(heights: Record<string, number>): number {
  let tallest = 0;
  for (const height of Object.values(heights)) {
    if (height > tallest) tallest = height;
  }
  return tallest;
}

/**
 * Extra headroom (px) the shortcut zone reserves on every side so magnified
 * tiles are not clipped by its own overflow-x scroll container (which clips
 * on both axes): the whole tile (icon plus a below-label) can grow by
 * `scale − 1` of its height upwards, and by half of that per horizontal
 * side — the same value covers both. 0 when the magnify effect is off.
 */
export function magnifyReserve(
  shortcuts: ShortcutMetrics,
  effects: EffectsConfig,
  cardTiles = false,
  measuredHeight?: number,
): number {
  const scale = magnifyScale(effects);
  if (scale <= 1) return 0;
  const icon = clampIconSize(shortcuts.iconSize);
  const cardPad = cardTiles ? 20 : 0;
  const estimated =
    (shortcuts.labels === "below"
      ? icon + LABEL_GAP_PX + clampLabelSize(shortcuts.labelSize)
      : icon) + cardPad;
  const height =
    measuredHeight !== undefined && measuredHeight > 0
      ? measuredHeight
      : estimated;
  return Math.ceil(height * (scale - 1));
}

/**
 * Apple-dock fisheye falloff: full `maxScale` at the cursor, a smooth
 * cosine falloff over `neighbors` item widths on each side, 1 outside.
 * With neighbors = 0 only the tile under the cursor scales (the influence
 * radius is half an item width).
 */
export function fisheyeScale(
  distance: number,
  itemWidth: number,
  maxScale: number,
  neighbors: number,
): number {
  if (itemWidth <= 0 || maxScale <= 1) return 1;
  const radius = itemWidth * (clampNeighbors(neighbors) + 0.5);
  const d = Math.abs(distance);
  if (d >= radius) return 1;
  const falloff = 0.5 * (1 + Math.cos((Math.PI * d) / radius));
  return 1 + (maxScale - 1) * falloff;
}
