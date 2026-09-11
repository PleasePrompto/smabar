/** Pure settings-panel logic: list reordering, tile toggles, clamps. */

import type { ThemeSummary } from "../../store/bar";

export function themeDisplayName(theme: ThemeSummary): string {
  return (
    theme.meta?.name ?? theme.name.charAt(0).toUpperCase() + theme.name.slice(1)
  );
}

/**
 * Returns a copy of `items` with the element at `from` moved to `to`.
 * Out-of-range indices (or from === to) return an unchanged copy — the
 * up/down buttons at the list edges are no-ops, never errors.
 */
export function moveItem<T>(
  items: readonly T[],
  from: number,
  to: number,
): T[] {
  const next = [...items];
  if (from < 0 || from >= items.length || to < 0 || to >= items.length) {
    return next;
  }
  const removed = next.splice(from, 1);
  next.splice(to, 0, ...removed);
  return next;
}

/**
 * Applies a reorder of the VISIBLE items to the full configured order:
 * hidden ids keep their slots, the visible ids are re-dealt into the
 * remaining slots in their new order. Ids in `visible` that are missing from
 * `all` are ignored — the caller derives both from the same registry.
 */
export function reorderWithHidden(
  all: readonly string[],
  visible: readonly string[],
  from: number,
  to: number,
): string[] {
  const next = moveItem(visible, from, to);
  const shown = new Set(visible);
  let taken = 0;
  return all.map((id) => {
    if (!shown.has(id)) return id;
    const replacement = next[taken];
    taken += 1;
    return replacement ?? id;
  });
}

/**
 * Toggles `id` in a list of switched-off ids (add when absent, else remove).
 * Serves both off-switches, which are deliberately separate lists: tile ids
 * in `pluginsHidden` (hidden, plugin keeps running) and plugin ids in
 * `pluginsDeactivated` (no process at all).
 */
export function toggleDisabled(
  disabled: readonly string[],
  id: string,
): string[] {
  return disabled.includes(id)
    ? disabled.filter((entry) => entry !== id)
    : [...disabled, id];
}

export const MAGNIFY_MIN = 1;
export const MAGNIFY_MAX = 1.6;
export const MAGNIFY_STEP = 0.05;
/** Matches `config::HoverMagnify::default().scale`. */
export const MAGNIFY_DEFAULT = 1.2;

/**
 * Clamps a hover-magnify scale into the supported 1.0–1.6 range; non-finite
 * input (an emptied number field) falls back to the config default.
 */
export function clampMagnifyScale(scale: number): number {
  if (!Number.isFinite(scale)) return MAGNIFY_DEFAULT;
  return Math.min(MAGNIFY_MAX, Math.max(MAGNIFY_MIN, scale));
}

export const ICON_SIZE_MIN = 16;
export const ICON_SIZE_MAX = 64;
export const ICON_SIZE_STEP = 2;
/** Matches `config::ShortcutsConfig::default().icon_size`. */
export const ICON_SIZE_DEFAULT = 24;

export const LABEL_SIZE_MIN = 9;
export const LABEL_SIZE_MAX = 16;
/** Matches `config::ShortcutsConfig::default().label_size`. */
export const LABEL_SIZE_DEFAULT = 12;

export const MARGIN_MIN = 0;
export const MARGIN_MAX = 64;
export const MARGIN_STEP = 2;
/** Matches `config::LayoutConfig::default().margin`. */
export const MARGIN_DEFAULT = 10;

export const NEIGHBORS_MIN = 0;
export const NEIGHBORS_MAX = 3;
/** Matches `config::HoverMagnify::default().neighbors`. */
export const NEIGHBORS_DEFAULT = 2;

/* Appearance fine-tuning sliders. They write `appearance.tokens` overrides
   (--sb-* custom properties applied after the theme); the defaults mirror
   themes/default.json, so "no override" and "override at default" render
   identically. */

export const GAP_MIN = 0;
export const GAP_MAX = 24;
/** Matches `--sb-tile-gap` / `--sb-shortcut-gap` in themes/default.json. */
export const GAP_DEFAULT = 4;

export const BAR_PAD_Y_MIN = 2;
export const BAR_PAD_Y_MAX = 20;
/** Matches `--sb-bar-pad-y` in themes/default.json. */
export const BAR_PAD_Y_DEFAULT = 6;
/** Bar content floor: default `--sb-bar-height` 52px minus 2 × 6px pad-y.
 * The whitespace slider keeps `--sb-bar-height` = floor + 2 × pad-y, so the
 * bar visibly shrinks and grows with the slider across its whole range. */
export const BAR_CONTENT_FLOOR_PX = 40;

export const BAR_RADIUS_MIN = 0;
export const BAR_RADIUS_MAX = 32;
/** Matches `--sb-bar-radius` in themes/default.json (0.75rem = 12px). */
export const BAR_RADIUS_DEFAULT = 12;

export const BAR_BORDER_MIN = 1;
export const BAR_BORDER_MAX = 6;
/** Matches `--sb-bar-border-width` in themes/default.json. */
export const BAR_BORDER_DEFAULT = 1;

/** Whole-UI zoom in percent. 100 = the sizes the themes ship. */
export const SCALE_MIN = 75;
export const SCALE_MAX = 150;
export const SCALE_DEFAULT = 100;

/** 0 removes the bar surface entirely; 100 is genuinely opaque, because the
 * surface tokens carry no alpha of their own any more (see themes/*.json). */
export const BAR_OPACITY_MIN = 0;
export const BAR_OPACITY_MAX = 100;
/** Matches `--sb-bar-opacity` in themes/default.json (percent). */
export const BAR_OPACITY_DEFAULT = 92;
/** Matches `--sb-flyout-opacity` in themes/default.json (percent). */
export const FLYOUT_OPACITY_DEFAULT = 97;

/**
 * Numeric leading value of a token (e.g. "12px" → 12, "62%" → 62). Without an
 * override the ACTIVE theme's value is read back from `:root` — the bundled
 * themes differ in gaps, bar height and opacity, so a hardcoded constant would
 * park every slider at default.json's value and make the first drag jump.
 * `fallback` is the last resort (no theme applied yet, e.g. in unit tests).
 */
export function tokenNumber(
  tokens: Record<string, string>,
  key: string,
  fallback: number,
): number {
  const raw = (tokens[key] ?? rootToken(key)).trim();
  const parsed = Number.parseFloat(raw);
  if (!Number.isFinite(parsed)) return fallback;
  // The sliders speak px at 100% scale; the tokens are rem so the global size
  // slider can scale them. Convert against the BASE, never the live root font
  // size — otherwise every slider would read a different number at each scale
  // and writing it back would freeze the value at that one scale.
  return raw.endsWith("rem") ? parsed * REM_BASE_PX : parsed;
}

/** Reference root font size the slider numbers are expressed in (scale 100%). */
export const REM_BASE_PX = 16;

/** px slider value → the rem string stored in `appearance.tokens`. */
export function pxToRem(px: number): string {
  return `${String(Number((px / REM_BASE_PX).toFixed(4)))}rem`;
}

/** The token as theme/apply.ts left it on `:root`; "" when unset. */
function rootToken(key: string): string {
  if (typeof document === "undefined") return "";
  return document.documentElement.style.getPropertyValue(key);
}

/**
 * Token writes of the vertical-whitespace slider: the pad drives the row
 * formula (metrics.rowHeightExpr) and the height floor follows it, so the
 * default (8) reproduces today's 56px bar exactly.
 */
export function whitespaceTokens(padY: number): Record<string, string> {
  const clamped = clampInt(
    padY,
    BAR_PAD_Y_MIN,
    BAR_PAD_Y_MAX,
    BAR_PAD_Y_DEFAULT,
  );
  return {
    "--sb-bar-pad-y": pxToRem(clamped),
    "--sb-bar-height": pxToRem(BAR_CONTENT_FLOOR_PX + 2 * clamped),
  };
}

/** Rounds into an integer range; non-finite input falls back to `fallback`. */
function clampInt(
  value: number,
  min: number,
  max: number,
  fallback: number,
): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, Math.round(value)));
}

/** Clamps a shortcut icon size into the supported 16–64 px range. */
export function clampIconSize(size: number): number {
  return clampInt(size, ICON_SIZE_MIN, ICON_SIZE_MAX, ICON_SIZE_DEFAULT);
}

/** Clamps a shortcut label font size into the supported 9–16 px range. */
export function clampLabelSize(size: number): number {
  return clampInt(size, LABEL_SIZE_MIN, LABEL_SIZE_MAX, LABEL_SIZE_DEFAULT);
}

/** Clamps the auto-width edge margin into the supported 0–64 px range. */
export function clampMargin(margin: number): number {
  return clampInt(margin, MARGIN_MIN, MARGIN_MAX, MARGIN_DEFAULT);
}

/** Clamps the fisheye neighbor span into the supported 0–3 range. */
export function clampNeighbors(neighbors: number): number {
  return clampInt(neighbors, NEIGHBORS_MIN, NEIGHBORS_MAX, NEIGHBORS_DEFAULT);
}

/**
 * The colour tokens the pickers own, and what each one drags along.
 *
 * Picking one colour has to leave a coherent bar, so a pick writes the
 * derived tokens too instead of asking the user for six values that must
 * agree with each other. `--sb-on-accent` is deliberately NOT derived here —
 * theme/apply.ts computes it from whatever accent ends up active, which also
 * covers themes and plugin branding.
 */
const ACCENT_GRADIENT =
  "linear-gradient(135deg, var(--sb-accent), var(--sb-accent-2))";

/** Primary accent plus the effects whose strength is based on it. */
export function accentPrimaryTokens(accent: string): Record<string, string> {
  return {
    "--sb-accent": accent,
    // Keep dependencies live: changing either base accent through a theme,
    // MCP, branding, or settings must update every derived accent surface.
    "--sb-accent-gradient": ACCENT_GRADIENT,
    "--sb-accent-glow":
      "0 2px 12px -2px color-mix(in srgb, var(--sb-accent) 35%, transparent)",
  };
}

/** Secondary accent without taking ownership of the primary override. */
export function accentSecondaryTokens(accent2: string): Record<string, string> {
  return {
    "--sb-accent-2": accent2,
    "--sb-accent-gradient": ACCENT_GRADIENT,
  };
}

/** Complete accent group, retained for resets and callers changing both ends. */
export function accentTokens(
  accent: string,
  accent2: string,
): Record<string, string> {
  return {
    ...accentPrimaryTokens(accent),
    ...accentSecondaryTokens(accent2),
  };
}

/**
 * Every neutral surface that should follow the picked background colour.
 * All results stay opaque; the independent bar/flyout opacity tokens own
 * bar transparency, so a colour can never put an invisible ceiling on
 * either opacity slider.
 */
export function surfaceTokens(color: string): Record<string, string> {
  const surface = "var(--sb-bar-bg)";
  return {
    "--sb-bar-bg": color,
    "--sb-flyout-bg": surface,
    "--sb-tile-bg": mixSurface(surface, 4),
    "--sb-tile-hover-bg": mixSurface(surface, 8),
    "--sb-inner-bg": mixSurface(surface, 5),
    "--sb-surface-2": mixSurface(surface, 9),
    "--sb-menu-bg": mixSurface(surface, 4),
    "--sb-menu-hover-bg":
      "color-mix(in srgb, var(--sb-menu-bg) 90%, var(--sb-text))",
    "--sb-overlay-bg": mixSurface(surface, 8),
    "--sb-window-bg": surface,
    "--sb-tooltip-bg": "var(--sb-flyout-bg)",
  };
}

/** Opaque tint toward the resolved text colour, expressed as a live alias. */
function mixSurface(color: string, textPercent: number): string {
  return `color-mix(in srgb, ${color} ${String(100 - textPercent)}%, var(--sb-text) ${String(textPercent)}%)`;
}

/**
 * Text plus its three muted steps. The steps fade toward transparent, the
 * same relationship ui-kit.css uses for accent surfaces, so they stay
 * correct on whatever background the text ends up over.
 */
export function textTokens(color: string): Record<string, string> {
  return {
    "--sb-text": color,
    "--sb-text-dim": "color-mix(in srgb, var(--sb-text) 74%, transparent)",
    "--sb-text-muted": "color-mix(in srgb, var(--sb-text) 62%, transparent)",
    "--sb-text-faint": "color-mix(in srgb, var(--sb-text) 45%, transparent)",
    "--sb-menu-text": "var(--sb-text)",
    "--sb-tooltip-text": "var(--sb-text)",
  };
}

/**
 * Every token the four colour pickers can write, derived from the builders
 * above rather than listed again — a new derived token is then covered by
 * "reset colours" for free.
 */
export const COLOR_TOKENS: readonly string[] = [
  ...Object.keys(accentTokens("", "")),
  ...Object.keys(surfaceTokens("")),
  ...Object.keys(textTokens("")),
];

/**
 * Client-side mirror of the core's `slugify_theme_name` (themes/io.rs):
 * lowercase, runs outside `[a-z0-9]` collapse to one dash, capped at 64
 * characters. The core stays the authority — this only powers live input
 * feedback and collision checks against the loaded theme list.
 */
export function slugifyThemeName(input: string): string | null {
  const slug = input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+/, "")
    .slice(0, 64)
    .replace(/-+$/, "");
  return slug === "" ? null : slug;
}

/** Theme name the core derives from a typed or dropped file path. */
export function slugifyThemePath(path: string): string | null {
  const stem =
    path
      .replace(/\\/g, "/")
      .split("/")
      .pop()
      ?.replace(/\.json$/i, "") ?? "";
  return slugifyThemeName(stem);
}
