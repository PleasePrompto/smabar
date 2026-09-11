/**
 * Applies the resolved theme (flat map of CSS custom properties) plus the
 * user's per-token overrides (`appearance.tokens`, written by the appearance
 * sliders) to the document root. Overrides win over the theme; both layers
 * are re-rendered together so switching either never leaves stale tokens.
 * Custom properties inherit through shadow-DOM boundaries, so plugin tiles
 * are themed automatically. The stylesheet keeps the default values as
 * var() fallbacks, so a browser-dev session without Tauri looks identical.
 */

import { readableTextOn, readableTextOnAll } from "./contrast";
import { canonicalizeManagedFonts, syncManagedThemeFonts } from "./fonts";

let themeTokens: Record<string, string> = {};
let overrideTokens: Record<string, string> = {};
let appliedKeys = new Set<string>();

/** Replace the theme layer (from `get_ui_state` / `theme-changed`). */
export function applyTheme(tokens: Record<string, string>): void {
  themeTokens = tokens;
  render();
}

/** Replace the override layer (from `appearance.tokens`). */
export function applyTokenOverrides(tokens: Record<string, string>): void {
  overrideTokens = tokens;
  render();
}

/** Set the merged tokens on `:root`; tokens applied earlier but missing now
 * are removed again so no switch ever leaves stale values behind. Only
 * custom-property names (`--*`) are applied. */
function render(): void {
  const style = document.documentElement.style;
  const merged = canonicalizeManagedFonts({
    ...themeTokens,
    ...overrideTokens,
  });
  const mergedKeys = new Set<string>();

  // Install the current inputs before resolving modern CSS colours. var(),
  // color-mix() and colour-space functions only become measurable once their
  // custom-property dependencies exist on :root.
  for (const [key, value] of Object.entries(merged)) {
    if (!key.startsWith("--")) continue;
    if (style.getPropertyValue(key) !== value) style.setProperty(key, value);
    mergedKeys.add(key);
  }
  for (const key of appliedKeys) {
    if (!mergedKeys.has(key)) style.removeProperty(key);
  }

  const derived = derive(merged);
  const next = new Set<string>();
  for (const [key, value] of Object.entries(derived)) {
    if (!key.startsWith("--")) continue;
    if (style.getPropertyValue(key) !== value) style.setProperty(key, value);
    next.add(key);
  }
  appliedKeys = next;
  syncManagedThemeFonts(derived);
}

/**
 * Repairs text tokens that would be unreadable on the surface they sit on,
 * and leaves every legible declaration alone (see theme/contrast.ts). This
 * is what keeps a plugin from painting light text on a light theme once the
 * user picks their own accent: `--sb-on-accent` is what every accent surface
 * in the kit derives its text steps from.
 *
 * `--sb-text` is only derived while the user has NOT picked a text colour —
 * an explicit choice in the settings is an instruction, not a suggestion.
 */
function derive(merged: Record<string, string>): Record<string, string> {
  const derived = { ...merged };
  const accents = [merged["--sb-accent"], merged["--sb-accent-2"]].filter(
    (color): color is string => color !== undefined && color !== "",
  );
  const onAccent = readableTextOnAll(accents, merged["--sb-on-accent"]);
  if (onAccent !== null) derived["--sb-on-accent"] = onAccent;
  if (overrideTokens["--sb-text"] === undefined) {
    const text = readableTextOn(merged["--sb-bar-bg"], merged["--sb-text"]);
    if (text !== null) derived["--sb-text"] = text;
  }
  return derived;
}
