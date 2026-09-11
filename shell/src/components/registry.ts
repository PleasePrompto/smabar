import { useSmabar } from "../store/bar";
import type { PluginTileDef } from "../plugins/PluginContent";

export interface PluginTileDefinition {
  id: string;
  pluginId: string;
  iconDataUrl?: string | null;
  tile: PluginTileDef;
  meta: {
    /** Manifest display name, rendered through t() for locale overrides. */
    name: string;
  };
}

const registry = new Map<string, PluginTileDefinition>();

/** Register a tile; re-registering the same id replaces the entry. */
export function registerTile(definition: PluginTileDefinition): void {
  registry.set(definition.id, definition);
}

/**
 * Removes one plugin's tiles, keeping the ids in `keep`. Returns the removed
 * TILE ids (the `plugin:<pluginId>:` prefix stripped) so the caller can drop
 * their cached HTML too.
 *
 * `keep` is what a re-registration declares: a manifest edit may list FEWER
 * tiles than before, and without this sweep the dropped ones keep rendering
 * their last HTML until the app restarts. Survivors are kept in place rather
 * than deleted and re-added so a hot reload cannot move them in the registry's
 * insertion order.
 */
export function unregisterPluginTiles(
  pluginId: string,
  keep?: ReadonlySet<string>,
): string[] {
  const prefix = `plugin:${pluginId}:`;
  const removed: string[] = [];
  for (const id of [...registry.keys()]) {
    if (!id.startsWith(prefix)) continue;
    const tileId = id.slice(prefix.length);
    if (keep?.has(tileId)) continue;
    registry.delete(id);
    removed.push(tileId);
  }
  return removed;
}

/**
 * Applies the configured order: ids in `order` come first (in list order),
 * everything else follows in the given (registration) order.
 */
export function sortTiles(
  tiles: PluginTileDefinition[],
  order: string[],
): PluginTileDefinition[] {
  if (order.length === 0) return tiles;
  const rank = new Map(order.map((id, index) => [id, index]));
  const listed = tiles
    .filter((tile) => rank.has(tile.id))
    .sort((a, b) => (rank.get(a.id) ?? 0) - (rank.get(b.id) ?? 0));
  const rest = tiles.filter((tile) => !rank.has(tile.id));
  return [...listed, ...rest];
}

/**
 * Drops the tiles listed in `pluginsHidden`. Kept separate from
 * {@link getTiles} on purpose: lifecycle code (plugin unregistration)
 * must still see disabled tiles.
 */
export function visibleTiles(
  tiles: PluginTileDefinition[],
  disabled: string[],
): PluginTileDefinition[] {
  if (disabled.length === 0) return tiles;
  const hidden = new Set(disabled);
  return tiles.filter((tile) => !hidden.has(tile.id));
}

/**
 * All registered tiles in registration order, reordered by the store's
 * configured pluginOrder. Components rendering this must subscribe to both
 * registryVersion and pluginOrder.
 */
export function getTiles(): PluginTileDefinition[] {
  return sortTiles([...registry.values()], useSmabar.getState().pluginOrder);
}

export function getTile(id: string): PluginTileDefinition | undefined {
  return registry.get(id);
}
