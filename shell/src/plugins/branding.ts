import type { CSSProperties } from "react";

import { readableTextOn } from "../theme/contrast";
import type { PluginTileDef } from "./PluginContent";

/** Inline token overrides for a branded tile host (per-tile accent). */
export type BrandingStyle = CSSProperties &
  Partial<
    Record<
      | "--sb-accent"
      | "--sb-accent-2"
      | "--sb-on-accent"
      | "--sb-accent-gradient"
      | "--sb-accent-glow",
      string
    >
  >;

/**
 * Builds the host-level custom-property overrides declared by a tile's
 * manifest branding. Custom properties inherit into the shadow tree, so the
 * kit and every var(--sb-accent*) usage brand along. Undefined without
 * `accent` — themes stay in charge then.
 *
 * `--sb-on-accent` is always set, not only when the manifest declares
 * `accentFg`: every accent surface in the kit derives its text steps from it
 * (ui-kit.css), so a light brand colour left it painting white on white. A
 * declared `accentFg` still wins as long as it is legible.
 */
export function brandingStyle(tile: PluginTileDef): BrandingStyle | undefined {
  if (tile.accent === undefined || tile.accent === "") return undefined;
  const accent2 = tile.accent2 ?? tile.accent;
  const declared =
    tile.accentFg === undefined || tile.accentFg === ""
      ? undefined
      : tile.accentFg;
  const style: BrandingStyle = {
    "--sb-accent": tile.accent,
    "--sb-accent-2": accent2,
    "--sb-accent-gradient":
      "linear-gradient(135deg, var(--sb-accent), var(--sb-accent-2))",
    "--sb-accent-glow":
      "0 2px 12px -2px color-mix(in srgb, var(--sb-accent) 35%, transparent)",
  };
  const onAccent = readableTextOn(tile.accent, declared) ?? declared;
  if (onAccent !== undefined) style["--sb-on-accent"] = onAccent;
  return style;
}

/**
 * Branding by `<pluginId>/<tileId>`, so surfaces that only know those ids
 * can brand too. Popups arrive through the popup queue as separate plugin and
 * tile ids rather than a registry id.
 */
const brandings = new Map<string, BrandingStyle>();

const brandingKey = (pluginId: string, tileId: string) =>
  `${pluginId}/${tileId}`;

/** Records (or clears) one tile's branding; called on plugin registration. */
export function setBranding(
  pluginId: string,
  tileId: string,
  style: BrandingStyle | undefined,
): void {
  const key = brandingKey(pluginId, tileId);
  if (style === undefined) brandings.delete(key);
  else brandings.set(key, style);
}

/** Branding for a tile addressed by id; undefined when unbranded. */
export function brandingFor(
  pluginId: string,
  tileId: string,
): BrandingStyle | undefined {
  return brandings.get(brandingKey(pluginId, tileId));
}
