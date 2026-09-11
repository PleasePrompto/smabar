import type { ConfigWrite } from "./persist";

/**
 * Defaults mirror the Rust config model and are written one path at a time.
 *
 * One constant per settings section, and every path lives in exactly one of
 * them: a section's reset must restore what that section SHOWS, no more and
 * no less. The `*_TOKENS` lists say which `appearance.tokens` overrides a
 * section owns — they are dropped from the map by `clearTokens` instead of
 * emptying it, so resetting the bar cannot discard the colours under Design.
 */
export const BAR_DEFAULTS = [
  { path: "layout.monitor", value: null },
  { path: "layout.position", value: "bottom" },
  { path: "layout.variant", value: "split" },
  { path: "layout.dividerRatio", value: 0.5 },
  { path: "layout.primaryZone", value: "plugins" },
  { path: "layout.width", value: "auto" },
  { path: "layout.margin", value: 10 },
  { path: "layout.maxWidth", value: 0 },
  { path: "layout.behavior", value: "reserve" },
  { path: "layout.yieldToFullscreen", value: true },
  { path: "zOrder", value: "top" },
  { path: "popups.enabled", value: true },
  { path: "popups.position", value: "bottom-right" },
] as const satisfies readonly ConfigWrite[];

/** Overall size and the bar's own height. */
export const BAR_TOKENS = [
  "--sb-scale",
  "--sb-bar-pad-y",
  "--sb-bar-height",
] as const;

export const SHORTCUTS_DEFAULTS = [
  { path: "shortcuts.labels", value: "hidden" },
  { path: "shortcuts.iconSize", value: 24 },
  { path: "shortcuts.labelSize", value: 12 },
  { path: "shortcuts.pinned", value: [] },
  { path: "appearance.shortcutAlign", value: "center" },
  { path: "effects.hoverMagnify.enabled", value: true },
  { path: "effects.hoverMagnify.scale", value: 1.2 },
  { path: "effects.hoverMagnify.neighbors", value: 2 },
] as const satisfies readonly ConfigWrite[];

export const SHORTCUTS_TOKENS = ["--sb-shortcut-gap"] as const;

/**
 * Reset shows every tile again AND switches every plugin back on. Both are
 * reversible, so a reset can undo them; deleting a plugin is not, which is
 * why nothing here touches installed files.
 */
export const PLUGINS_DEFAULTS = [
  { path: "pluginsHidden", value: [] },
  { path: "pluginsDeactivated", value: [] },
  { path: "pluginOrder", value: [] },
  { path: "appearance.tileChrome", value: "card" },
  { path: "appearance.pluginAlign", value: "center" },
  { path: "effects.hoverPeek.enabled", value: true },
  { path: "effects.hoverPeek.delayMs", value: 400 },
] as const satisfies readonly ConfigWrite[];

export const PLUGINS_TOKENS = ["--sb-tile-gap"] as const;

export const DESIGN_DEFAULTS = [
  { path: "theme", value: "default" },
  { path: "themeExportDir", value: "" },
  { path: "appearance.barChrome", value: "card" },
] as const satisfies readonly ConfigWrite[];

/** Surface shape and opacity; colours are added from `COLOR_TOKENS`. */
export const DESIGN_TOKENS = [
  "--sb-bar-radius",
  "--sb-bar-border-width",
  "--sb-bar-opacity",
  "--sb-flyout-opacity",
] as const;

export const SYSTEM_DEFAULTS = [
  { path: "language", value: "en" },
  { path: "mcp.enabled", value: true },
  { path: "mcp.port", value: 7627 },
  { path: "rendering", value: "auto" },
] as const satisfies readonly ConfigWrite[];
