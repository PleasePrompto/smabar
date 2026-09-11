// Pure bar-state helpers and their constants — no store, no React, so the
// clamping rules stay testable on their own. Consumers import them from
// "../store/bar", which re-exports everything here.

import type {
  EffectsConfig,
  FlyoutDirection,
  FlyoutRect,
  ResolvedShortcut,
  ShortcutConfigEntry,
} from "./types";

/** Fallback when the configured divider ratio is not a number. */
export const DIVIDER_DEFAULT = 0.5;
export const HOVER_PEEK_DELAY_DEFAULT_MS = 400;
export const HOVER_PEEK_DELAY_MIN_MS = 100;
export const HOVER_PEEK_DELAY_MAX_MS = 2_000;
export const BAR_MAX_WIDTH_MIN_PX = 400;

/** Localizes built-in special items unless the user supplied a label override. */
export function shortcutDisplayLabel(
  shortcut: ResolvedShortcut,
  entry: ShortcutConfigEntry | undefined,
  translate: (key: string) => string,
): string {
  if (entry?.label !== undefined) return shortcut.label;
  if (entry?.special === "computer") {
    return translate("settings.shortcuts.specialComputer");
  }
  if (entry?.special === "trash") {
    return translate("settings.shortcuts.specialTrash");
  }
  return shortcut.label;
}

/** Clamps the split divider ratio into the supported 0.15–0.85 range. */
export function clampDividerRatio(ratio: number): number {
  if (Number.isNaN(ratio)) return DIVIDER_DEFAULT;
  return Math.min(0.85, Math.max(0.15, ratio));
}

/** Effective hover-peek delay, rounded and clamped to 100–2000 ms. */
export function clampHoverPeekDelay(delayMs: number): number {
  if (!Number.isFinite(delayMs)) return HOVER_PEEK_DELAY_DEFAULT_MS;
  return Math.min(
    HOVER_PEEK_DELAY_MAX_MS,
    Math.max(HOVER_PEEK_DELAY_MIN_MS, Math.round(delayMs)),
  );
}

/**
 * Effective full-bar cap. Zero remains unlimited; non-zero values have a
 * usable minimum and never exceed the current viewport.
 */
export function clampMaxWidth(
  maxWidth: number,
  viewportWidth = Infinity,
): number {
  if (!Number.isFinite(maxWidth) || maxWidth <= 0) return 0;
  const requested = Math.max(BAR_MAX_WIDTH_MIN_PX, Math.round(maxWidth));
  if (!Number.isFinite(viewportWidth) || viewportWidth <= 0) return requested;
  return Math.min(requested, Math.floor(viewportWidth));
}

/** Effective hover-magnify scale: 1 when disabled, else clamped to 1.0–1.6. */
export function magnifyScale(effects: EffectsConfig): number {
  const { enabled, scale } = effects.hoverMagnify;
  if (!enabled || Number.isNaN(scale)) return 1;
  return Math.min(1.6, Math.max(1, scale));
}

/**
 * Opening direction from the trigger's position: a trigger in the upper
 * window half opens downwards, one in the lower half upwards.
 */
export function flyoutDirection(
  rect: FlyoutRect,
  viewportHeight: number,
): FlyoutDirection {
  return rect.top + rect.height / 2 < viewportHeight / 2 ? "down" : "up";
}

/**
 * Whether a measured anchor rect may position an overlay surface. Besides a
 * finite positive box, its center must remain inside the viewport: an
 * autohidden bar deliberately leaves a thin reveal strip visible even though
 * its tiles are no longer interactive. Every rect-anchored surface refuses to
 * open on a rect failing this one shared check.
 */
export function isRenderableRect(
  rect: FlyoutRect,
  viewportWidth: number,
  viewportHeight: number,
): boolean {
  const validBox =
    Number.isFinite(rect.left) &&
    Number.isFinite(rect.top) &&
    Number.isFinite(rect.width) &&
    Number.isFinite(rect.height) &&
    rect.width > 0 &&
    rect.height > 0;
  if (
    !validBox ||
    !Number.isFinite(viewportWidth) ||
    !Number.isFinite(viewportHeight) ||
    viewportWidth <= 0 ||
    viewportHeight <= 0
  ) {
    return false;
  }
  const centerX = rect.left + rect.width / 2;
  const centerY = rect.top + rect.height / 2;
  return (
    centerX >= 0 &&
    centerX < viewportWidth &&
    centerY >= 0 &&
    centerY < viewportHeight
  );
}
