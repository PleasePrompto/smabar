/**
 * Resolves what a right-click actually hit.
 *
 * The walk goes over the event's COMPOSED path from the innermost node
 * outwards: plugin markup lives in open shadow roots, where `event.target`
 * is retargeted to the host and would hide the element the plugin declared
 * its menu on (same reason the data-action dispatch in PluginContent uses
 * composedPath).
 *
 * Marker attributes (single additive lines at the tile markup):
 *   data-shortcut-id    one pinned shortcut or separator (ShortcutItem)
 *   data-tile-id      one registered tile (PluginTile / ShadowHost)
 *   data-plugin-id +
 *   data-plugin-tile  a plugin's shadow host (ShadowHost)
 *   data-bar-root       a bar row (empty bar surface)
 */

import { CONTEXT_ITEMS_ATTR } from "./pluginMenu";

export const SHORTCUT_ID_ATTR = "data-shortcut-id";
export const TILE_ID_ATTR = "data-tile-id";
export const PLUGIN_ID_ATTR = "data-plugin-id";
export const PLUGIN_TILE_ATTR = "data-plugin-tile";
export const BAR_ROOT_ATTR = "data-bar-root";

export interface PluginContextTarget {
  kind: "plugin";
  pluginId: string;
  tileId: string;
  /** Raw data-context-items value of the innermost declaring element. */
  spec: string;
}

export interface ShortcutContextTarget {
  kind: "shortcut";
  id: string;
}

export interface TileContextTarget {
  kind: "tile";
  id: string;
}

export interface BarContextTarget {
  kind: "bar";
}

export type ContextTarget =
  | PluginContextTarget
  | ShortcutContextTarget
  | TileContextTarget
  | BarContextTarget;

/**
 * Text fields keep the webview's native menu: it is the only cut/copy/paste
 * affordance a mouse-only user has inside the bar.
 */
export function isEditableTarget(path: readonly EventTarget[]): boolean {
  for (const hop of path) {
    if (hop instanceof HTMLInputElement || hop instanceof HTMLTextAreaElement) {
      return true;
    }
    if (hop instanceof HTMLElement && hop.isContentEditable) return true;
  }
  return false;
}

/**
 * Every area that could own a menu for this click, INNERMOST FIRST. The
 * caller takes the first candidate that actually produces items, which is
 * what makes the fallbacks work without extra branching: a plugin element
 * whose `data-context-items` is broken falls through to its tile, a
 * tile tile without a menu to the bar surface.
 */
export function resolveContextTargets(
  path: readonly EventTarget[],
): ContextTarget[] {
  const targets: ContextTarget[] = [];
  let spec: string | null = null;
  for (const hop of path) {
    if (!(hop instanceof HTMLElement)) continue;
    spec ??= hop.getAttribute(CONTEXT_ITEMS_ATTR);
    // Marker attributes inside plugin shadow markup are untrusted data-*.
    // Only shell-owned elements in the document root may identify plugins,
    // tiles, shortcuts, or the bar itself.
    const shellOwned = hop.getRootNode() === hop.ownerDocument;

    const pluginId = hop.getAttribute(PLUGIN_ID_ATTR);
    const pluginPlugin = hop.getAttribute(PLUGIN_TILE_ATTR);
    if (
      shellOwned &&
      spec !== null &&
      pluginId !== null &&
      pluginPlugin !== null
    ) {
      targets.push({ kind: "plugin", pluginId, tileId: pluginPlugin, spec });
      spec = null;
    }
    if (!shellOwned) continue;
    const shortcutId = hop.getAttribute(SHORTCUT_ID_ATTR);
    if (shortcutId !== null) targets.push({ kind: "shortcut", id: shortcutId });
    const tileId = hop.getAttribute(TILE_ID_ATTR);
    if (tileId !== null) targets.push({ kind: "tile", id: tileId });
    if (hop.hasAttribute(BAR_ROOT_ATTR)) targets.push({ kind: "bar" });
  }
  return targets;
}
