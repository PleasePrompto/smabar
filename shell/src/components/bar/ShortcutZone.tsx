import { useLayoutEffect, useState, type CSSProperties } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import {
  useSmabar,
  type LabelMode,
  type ResolvedShortcut,
  type ShortcutConfigEntry,
  shortcutDisplayLabel,
} from "../../store/bar";
import {
  clampIconSize,
  clampLabelSize,
  moveItem,
  pxToRem,
} from "../settings/model";
import { magnifyReserve } from "./metrics";
import { useDragReorder } from "./useDragReorder";
import { useFisheye } from "./useFisheye";
import { useZoneScroll } from "./useZoneScroll";
import { reportError } from "../../ipc/log";

function launchShortcut(id: string): void {
  // Same path the context menu's "Open" entry uses; outside the Tauri
  // window the fixture accepts and drops the launch.
  void call("launch_shortcut", { id }).catch(reportError);
}

/**
 * Commits a dragged pin through the SAME path the settings panel's move
 * buttons use (`update_config shortcuts.pinned` with the raw entries). The
 * store is updated first so the dock does not flicker while the roundtrip
 * runs, and restored when the write is rejected.
 */
function persistOrder(from: number, to: number): void {
  const store = useSmabar.getState();
  const previous = store.shortcuts;
  const entries = moveItem(previous.entries, from, to);
  store.setShortcuts({
    ...previous,
    pinned: moveItem(previous.pinned, from, to),
    entries,
  });
  call("update_config", { path: "shortcuts.pinned", value: entries }).catch(
    (error: unknown) => {
      useSmabar.getState().setShortcuts(previous);
      reportError(error);
    },
  );
}

/** Zone-level style carrying the shortcut display CSS vars (bar.css). */
type ZoneStyle = CSSProperties &
  Partial<
    Record<
      | "--sb-shortcut-icon-size"
      | "--sb-shortcut-label-size"
      | "--sb-magnify-origin",
      string
    >
  >;

/**
 * The dock zone: pinned app shortcuts as rounded icon tiles with a
 * configurable label mode (right | below | hidden) and icon/label sizes.
 * Hovering runs the fisheye magnify (useFisheye — transform only, the row
 * never reflows); clicking opens the application, file or folder.
 */
export function ShortcutZone({ style }: { style?: CSSProperties }) {
  const shortcuts = useSmabar((s) => s.shortcuts);
  const effects = useSmabar((s) => s.effects);
  const position = useSmabar((s) => s.layout.position);
  const align = useSmabar((s) => s.appearance.shortcutAlign);
  const cardTiles = useSmabar((s) => s.appearance.tileChrome === "card");
  const dropActive = useSmabar((s) => s.dropActive);
  const scrollRef = useZoneScroll();
  const [tileHeight, setTileHeight] = useState<number>();

  // Theme padding and the global rem scale both affect the real tile height.
  // Observe the rendered tile instead of duplicating that CSS math in JS.
  useLayoutEffect(() => {
    const zone = scrollRef.current;
    if (zone === null) return;
    const tile = zone.querySelector<HTMLElement>(".shortcut-tile");
    if (tile === null) {
      setTileHeight(undefined);
      return;
    }
    const update = () => {
      setTileHeight(tile.offsetHeight || undefined);
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(tile);
    return () => {
      observer.disconnect();
    };
  }, [cardTiles, scrollRef, shortcuts.labels, shortcuts.pinned]);

  // Headroom for magnified tiles: the zone is an overflow-x scroll
  // container, so it clips on BOTH axes — instead the zone's box grows past
  // the row vertically (negative margin + padding keep the content in
  // place) and is padded horizontally, so scaled tiles stay unclipped where
  // the zone meets its neighbour (the last pin before the tile zone was
  // cut off). Horizontally the reserve is generous: a tile grows by half of
  // `scale − 1` per side, the reserve covers a full one. useFisheye
  // subtracts it on all four sides: that headroom is outside the bar row and
  // outside the input shape, so it must not read as "hovering the dock".
  const reserve = magnifyReserve(shortcuts, effects, cardTiles, tileHeight);
  useFisheye(scrollRef, effects, reserve);
  // One delegated listener for the whole dock — the pins (separators
  // included) reorder by long-press drag, exactly like the settings panel's
  // move buttons.
  useDragReorder(scrollRef, {
    items: shortcuts.pinned.map((pin) => pin.id),
    onReorder: persistOrder,
    reserve,
  });
  const zoneStyle: ZoneStyle = {
    gap: "var(--sb-shortcut-gap, 6px)",
    // rem, so the global size slider scales the shortcut icons and labels
    // along with everything else; the config keeps its px numbers.
    "--sb-shortcut-icon-size": pxToRem(clampIconSize(shortcuts.iconSize)),
    "--sb-shortcut-label-size": pxToRem(clampLabelSize(shortcuts.labelSize)),
    // Tiles grow away from the docked edge (Apple dock feel).
    "--sb-magnify-origin": position === "top" ? "center top" : "center bottom",
    ...(reserve > 0 && {
      height: `calc(100% + ${String(2 * reserve)}px)`,
      marginBlock: `${String(-reserve)}px`,
      paddingBlock: `${String(reserve)}px`,
      paddingInline: `${String(reserve)}px`,
    }),
    ...style,
  };

  return (
    <div
      ref={scrollRef}
      // data-shortcut-zone: hit-test anchor for file drag&drop (dragDrop.ts).
      data-shortcut-zone
      // data-zone-align drives justify-content in bar.css (safe center /
      // safe flex-end, so an overflowing zone stays scrollable to its start).
      data-zone-align={align}
      // items-stretch: dock buttons are as tall as the tile cards beside
      // them, so both zones read as one row of equal cards.
      className={`zone-scroll flex h-full min-w-0 flex-1 items-stretch ${
        dropActive ? "zone-drop-target" : ""
      }`}
      style={zoneStyle}
    >
      {shortcuts.pinned.length === 0 ? (
        // self-center: the stretched zone would otherwise pin this one-line
        // text to its top edge (tiles center their own content).
        <span className="self-center px-2 text-xs whitespace-nowrap text-faint">
          {t("shortcuts.empty")}
        </span>
      ) : (
        shortcuts.pinned.map((shortcut, index) => (
          <ShortcutItem
            key={shortcut.id}
            shortcut={shortcut}
            entry={shortcuts.entries[index]}
            labels={shortcuts.labels}
          />
        ))
      )}
    </div>
  );
}

export function ShortcutItem({
  shortcut,
  entry,
  labels,
}: {
  shortcut: ResolvedShortcut;
  entry?: ShortcutConfigEntry;
  labels: LabelMode;
}) {
  const label = shortcutDisplayLabel(shortcut, entry, t);
  // Dock-button look: with tileChrome "card" a pin carries the same card
  // material as the tile tiles (padding/radius live in bar.css).
  const card = useSmabar((s) => s.appearance.tileChrome === "card");
  // Website pins carry several well-known icon URLs (touch icon first, then
  // the favicon): a candidate can 404, be a soft-404 HTML page, or be
  // unreachable offline, so a failed load walks to the next one and an
  // exhausted list ends at the initial-letter tile. The pin id changes with
  // its source, so a re-pinned shortcut mounts fresh and retries.
  // Remember failed URLs, not their positions: a freshly cached data URI can
  // arrive before the old candidates after those candidates already failed.
  const [failedIcons, setFailedIcons] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const icon = shortcut.icons.find((source) => !failedIcons.has(source));

  if (shortcut.separator) {
    return (
      <span
        className="shortcut-separator shrink-0"
        aria-hidden="true"
        // Right-clickable like every other pin (ContextMenuLayer).
        data-shortcut-id={shortcut.id}
      />
    );
  }

  // gap-1 in below mode must match LABEL_GAP_PX in metrics.ts.
  const arrangement =
    labels === "below" ? "flex-col justify-center gap-1" : "gap-1.5";
  return (
    <button
      className={`shortcut-tile flex shrink-0 items-center ${arrangement} ${
        card ? "surface-tile surface-tile-hover" : ""
      }`}
      data-tile-chrome={card ? "card" : "flat"}
      onClick={(e) => {
        e.stopPropagation();
        launchShortcut(shortcut.id);
      }}
      aria-label={label}
      // Themed tooltip instead of the native GTK one (TooltipLayer); the
      // accessible name stays on aria-label above.
      data-sb-tooltip={label}
      data-shortcut-id={shortcut.id}
    >
      {icon !== undefined ? (
        <img
          src={icon}
          alt=""
          className="shortcut-icon object-contain"
          draggable={false}
          onError={() => {
            setFailedIcons((current) =>
              current.has(icon) ? current : new Set(current).add(icon),
            );
          }}
        />
      ) : (
        <span className="shortcut-icon shortcut-initial flex items-center justify-center">
          {label.charAt(0).toUpperCase()}
        </span>
      )}
      {labels !== "hidden" && (
        <span className="shortcut-label whitespace-nowrap text-dim">
          {label}
        </span>
      )}
    </button>
  );
}
