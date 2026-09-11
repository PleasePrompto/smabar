/**
 * The row model of the ONE tiles list — pure, so the product decision is
 * testable without a DOM.
 *
 * The list has to show two things the user experiences as one. A running
 * plugin contributes a row per tile it registered, in bar order. A plugin
 * that is switched off contributes NO tiles at all (deactivating
 * unregisters them), so without a row of its own it would vanish from the
 * only place it can be switched back on.
 *
 * Hence the shape: reorderable tile rows first, in bar order, then a tail
 * of plugins that have no tile in the bar. The tail is deliberately at the
 * end and deliberately not reorderable — a switched-off plugin has no place
 * in the bar's order, and keeping the reorderable rows contiguous and leading
 * is what lets the drag map a row index straight onto an order index.
 */
import type { InstalledPlugin } from "../../store/types";

/** What a row needs from the registry; keeps this module DOM- and React-free. */
export interface PluginTileEntry {
  id: string;
  meta: { name: string };
}

/**
 * Where the plugin behind a row came from and what the Community Store
 * knows about it. Null on a tile row until `list_plugins` has answered.
 */
export type PluginProvenance = Pick<
  InstalledPlugin,
  "origin" | "version" | "update" | "modified" | "blocked"
>;

/** A tile that is registered and therefore has a place in the bar's order. */
export interface PluginTileRowModel {
  kind: "tile";
  /** The `plugin:<pluginId>:<tileId>` id — also the React key. */
  id: string;
  pluginId: string;
  /** Manifest name, still to be run through `t()`. */
  name: string;
  /** Position in the full tile order; what the reorder writes back. */
  orderIndex: number;
  /** Hidden from the bar, but its plugin keeps running. */
  hidden: boolean;
  /** How many rows this same plugin owns — 1 for every bundled plugin today. */
  siblings: number;
  provenance: PluginProvenance | null;
}

/** An installed plugin with nothing in the bar: switched off, or broken. */
export interface PluginRowModel {
  kind: "plugin";
  id: string;
  pluginId: string;
  name: string;
  state: "off" | "failed";
  error?: string;
  provenance: PluginProvenance;
}

export type ListRow = PluginTileRowModel | PluginRowModel;

/**
 * The plugin behind a namespaced tile id.
 *
 * Every registry entry is plugin-owned, so any other shape is a programming
 * error rather than a second supported id format.
 */
export function pluginOfTile(tileId: string): string {
  const parts = tileId.split(":");
  const pluginId = parts[1];
  if (
    parts[0] !== "plugin" ||
    parts.length < 3 ||
    pluginId === undefined ||
    pluginId === ""
  ) {
    throw new Error(`invalid plugin tile id: ${tileId}`);
  }
  return pluginId;
}

/**
 * Builds the rows.
 *
 * `tiles` comes from `getTiles()` — every REGISTERED tile in bar order,
 * hidden ones included (they keep their slot and stay reorderable, they are
 * only dimmed). `installed` comes from `list_plugins`, which is the only
 * source that also knows about plugins with no process.
 */
export function buildRows(
  tiles: readonly PluginTileEntry[],
  installed: readonly InstalledPlugin[],
  hidden: readonly string[],
): ListRow[] {
  const ownedTiles = tiles.map((tile) => ({
    tile,
    pluginId: pluginOfTile(tile.id),
  }));
  const counts = new Map<string, number>();
  for (const { pluginId } of ownedTiles) {
    counts.set(pluginId, (counts.get(pluginId) ?? 0) + 1);
  }
  const provenanceOf = new Map(
    installed.map((plugin) => [plugin.id, provenance(plugin)]),
  );

  const rows: ListRow[] = ownedTiles.map(({ tile, pluginId }, index) => ({
    kind: "tile",
    id: tile.id,
    pluginId,
    name: tile.meta.name,
    orderIndex: index,
    hidden: hidden.includes(tile.id),
    siblings: counts.get(pluginId) ?? 1,
    provenance: provenanceOf.get(pluginId) ?? null,
  }));

  // The tail: anything installed that put no row above. A plugin is listed
  // once, however many tiles its manifest declares — it has none in the
  // bar right now, which is the whole reason it needs a row.
  for (const plugin of installed) {
    if (counts.has(plugin.id)) continue;
    rows.push({
      kind: "plugin",
      id: `plugin:${plugin.id}`,
      pluginId: plugin.id,
      name: plugin.name ?? plugin.id,
      state: plugin.status === "failed" ? "failed" : "off",
      ...(plugin.error != null && plugin.error !== ""
        ? { error: plugin.error }
        : {}),
      provenance: provenance(plugin),
    });
  }
  return rows;
}

function provenance(plugin: InstalledPlugin): PluginProvenance {
  const { origin, version, update, modified, blocked } = plugin;
  return { origin, version, update, modified, blocked };
}

/** The rows a drag may move: the leading, contiguous tile rows. */
export function reorderableIds(rows: readonly ListRow[]): string[] {
  return rows.filter((row) => row.kind === "tile").map((row) => row.id);
}
