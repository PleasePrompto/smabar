import { Blocks, GripVertical } from "lucide-react";
import { useRef } from "react";

import { t } from "../../i18n/t";
import { useSmabar } from "../../store/bar";
import { getTile, getTiles } from "../registry";
import { PluginIcon } from "../../plugins/PluginIcon";
import { MoveButtons } from "./controls";
import { moveItem } from "./model";
import { setConfig } from "./persist";
import { OriginBadge } from "./StoreBadge";
import { useListReorder } from "./useListReorder";
import type { PluginManagement } from "./usePluginManagement";
import { PluginActions } from "./PluginActions";
import { buildRows, reorderableIds, type ListRow } from "./pluginListModel";

/** Compact, reorderable overview; the cards below share its management state. */
export function PluginList({ management }: { management: PluginManagement }) {
  useSmabar((state) => state.registryVersion);
  useSmabar((state) => state.pluginOrder);
  const hidden = useSmabar((state) => state.pluginsHidden);
  const deactivated = useSmabar((state) => state.pluginsDeactivated);
  const listRef = useRef<HTMLDivElement>(null);
  const installed = management.installed ?? [];

  const tiles = getTiles();
  const rows = buildRows(tiles, installed, hidden);
  const orderIds = reorderableIds(rows);

  // One commit path for both gestures: the arrows and the drag call this.
  // No useCallback — the hook reads it through a ref, so a fresh identity
  // costs nothing.
  const move = (from: number, to: number) => {
    setConfig("pluginOrder", moveItem(orderIds, from, to));
  };

  useListReorder(listRef, { count: orderIds.length, onReorder: move });

  return (
    <div className="sb-list" ref={listRef}>
      {rows.map((row) => (
        <Row
          key={row.id}
          row={row}
          count={orderIds.length}
          off={deactivated.includes(row.pluginId)}
          onMove={move}
          management={management}
        />
      ))}
      {rows.length === 0 && management.installed !== null && (
        <div className="sb-faint">{t("settings.plugins.empty")}</div>
      )}
    </div>
  );
}

interface RowProps {
  row: ListRow;
  count: number;
  off: boolean;
  onMove: (from: number, to: number) => void;
  management: PluginManagement;
}

function Row(props: RowProps) {
  const { row, off, management } = props;
  const dimmed = off || row.kind === "plugin" || row.hidden;
  const plugin = management.installed?.find(
    (plugin) => plugin.id === row.pluginId,
  );
  const registered = getTile(row.id);
  const tile = registered?.tile ?? plugin?.tiles[0];
  return (
    <div
      className="sb-row"
      data-reorder-index={row.kind === "tile" ? row.orderIndex : undefined}
      style={dimmed ? { opacity: 0.55 } : undefined}
    >
      {row.kind === "tile" ? (
        <span
          className="settings-drag-handle"
          data-drag-handle
          aria-hidden="true"
        >
          <GripVertical size="1em" />
        </span>
      ) : (
        <span className="settings-drag-handle" aria-hidden="true" />
      )}
      <PluginIcon
        pluginId={row.pluginId}
        tileId={tile?.id ?? ""}
        svg={tile?.iconSvg}
        dataUrl={registered?.iconDataUrl ?? plugin?.iconDataUrl}
        style={{ fontSize: "1.5rem", flexShrink: 0 }}
        fallback={<Blocks size="0.75em" />}
      />
      <span className="settings-ellipsis" style={{ flex: 1, minWidth: 0 }}>
        {t(row.name)}
      </span>
      <OriginBadge provenance={row.provenance} />
      <State row={row} off={off} />
      {row.kind === "tile" && !off && (
        <MoveButtons
          index={row.orderIndex}
          count={props.count}
          onMove={props.onMove}
        />
      )}
      <PluginActions
        pluginId={row.pluginId}
        name={t(row.name)}
        tiles={row.kind === "tile" ? [{ id: row.id, name: row.name }] : []}
        tileCount={row.kind === "tile" ? row.siblings : 0}
        management={management}
        compact
      />
    </div>
  );
}

/** The one-word state, so "switched off" never looks like "broken". */
function State({ row, off }: { row: ListRow; off: boolean }) {
  if (row.kind === "plugin" && row.state === "failed") {
    return (
      <span className="sb-crit" title={row.error}>
        {t("settings.plugins.stateFailed")}
      </span>
    );
  }
  if (off || row.kind === "plugin") {
    return <span className="sb-faint">{t("settings.plugins.stateOff")}</span>;
  }
  return null;
}
