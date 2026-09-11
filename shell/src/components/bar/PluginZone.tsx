import type { CSSProperties } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { useSmabar } from "../../store/bar";
import { getTiles, visibleTiles } from "../registry";
import { reorderWithHidden } from "../settings/model";
import { useDragReorder } from "./useDragReorder";
import { useZoneScroll } from "./useZoneScroll";
import { reportError } from "../../ipc/log";
import { PluginContent } from "../../plugins/PluginContent";

/**
 * Commits a dragged tile through the SAME path the settings panel's move
 * buttons use (`update_config pluginOrder`). The bar shows only the enabled
 * tiles, so the drop is re-dealt into the full order — hidden tiles keep
 * their slots and reappear where the user left them. The store leads so the
 * row does not flicker, and rolls back when the write is rejected.
 */
function persistOrder(from: number, to: number): void {
  const store = useSmabar.getState();
  const previous = store.pluginOrder;
  const all = getTiles();
  const next = reorderWithHidden(
    all.map((tile) => tile.id),
    visibleTiles(all, store.pluginsHidden).map((tile) => tile.id),
    from,
    to,
  );
  store.setPluginOrder(next);
  call("update_config", { path: "pluginOrder", value: next }).catch(
    (error: unknown) => {
      useSmabar.getState().setPluginOrder(previous);
      reportError(error);
    },
  );
}

/** Zone-level style carrying the magnify-origin CSS var (bar.css). */
type ZoneStyle = CSSProperties & Partial<Record<"--sb-magnify-origin", string>>;

/**
 * The tile zone: a calm row of tile tiles from the registry, in the
 * configured order, minus the disabled ones. Nothing else — no action
 * slots, no status fakes.
 */
export function PluginZone({ style }: { style?: CSSProperties }) {
  // The registry itself is not reactive — subscribing to registryVersion
  // re-renders when plugin tiles (de)register, pluginOrder feeds
  // getTiles, pluginsHidden the visibility filter.
  useSmabar((s) => s.registryVersion);
  useSmabar((s) => s.pluginOrder);
  const disabled = useSmabar((s) => s.pluginsHidden);
  const align = useSmabar((s) => s.appearance.pluginAlign);
  const position = useSmabar((s) => s.layout.position);
  const scrollRef = useZoneScroll();

  const tiles = visibleTiles(getTiles(), disabled);
  useDragReorder(scrollRef, {
    items: tiles.map((tile) => tile.id),
    onReorder: persistOrder,
  });

  const zoneStyle: ZoneStyle = {
    gap: "var(--sb-tile-gap, 6px)",
    // Cards grow away from the docked edge (bar.css .plugin-lift).
    "--sb-magnify-origin": position === "top" ? "center top" : "center bottom",
    ...style,
  };

  return (
    <div
      ref={scrollRef}
      // items-stretch: every cover is as tall as the row's content box, so a
      // one-line tile and a two-line tile read as one strip of equal cards.
      className="zone-scroll flex h-full min-w-0 flex-1 items-stretch"
      // data-zone-align drives justify-content in bar.css (safe center /
      // safe flex-end, so an overflowing zone stays scrollable to its start).
      data-zone-align={align}
      // data-plugin-zone: bar.css reserves effect headroom on it so a
      // lifted/magnified card is not clipped by this scroll container.
      data-plugin-zone
      style={zoneStyle}
    >
      {tiles.length === 0 ? (
        <span className="px-2 text-xs whitespace-nowrap text-faint">
          {t("plugins.empty")}
        </span>
      ) : (
        tiles.map((tile) => <PluginContent key={tile.id} definition={tile} />)
      )}
    </div>
  );
}
