import { Layers } from "lucide-react";
import { type CSSProperties, type MouseEvent, type ReactNode } from "react";

import { t } from "../../i18n/t";
import { setOverlayRowOpen } from "../../ipc/overlay";
import {
  clampDividerRatio,
  clampMaxWidth,
  useSmabar,
  type ZoneKind,
} from "../../store/bar";
import { BAR } from "../../styles/layers";
import { clampMargin } from "../settings/model";
import { SettingsButton } from "../settings/SettingsButton";
import { LegalGateTile } from "./LegalGateTile";
import { coverRowHeightExpr, rowHeightExpr, tallestCover } from "./metrics";
import { ShortcutZone } from "./ShortcutZone";
import { useAutohide } from "./useAutohide";
import { PluginZone } from "./PluginZone";
import { ZoneContent } from "./ZoneContent";
import { ZoneDivider } from "./ZoneDivider";

const rowStyle: CSSProperties = {
  height: "var(--sb-bar-row-height, var(--sb-bar-height))",
  paddingInline: "var(--sb-bar-pad)",
  // Real breathing room: without it a card tile as tall as its zone sits
  // flush against the bar edge. rowHeightExpr already budgets this padding
  // into the row height, so the content area never drops below the icons.
  paddingBlock: "var(--sb-bar-pad-y)",
  gap: "var(--sb-bar-gap)",
};

function BarRow({
  children,
  hidden = false,
  fit = false,
  captureTarget,
  onMiddleClick,
}: {
  children: ReactNode;
  /**
   * Content-wide and centred instead of spanning the dock: the solo
   * layout's visible row while its hidden sibling still holds the dock at
   * the wider of the two, so the pill hugs its content and the window
   * never resizes on toggle.
   */
  fit?: boolean;
  /**
   * Pre-rendered but invisible: the solo layout's second row keeps its
   * layout (so the window never resizes) and its running tiles, and
   * simply appears when toggled. Inert and without an input region, so the
   * transparent space is neither focusable nor clickable.
   */
  hidden?: boolean;
  captureTarget?: "overlay";
  onMiddleClick?: () => void;
}) {
  const barChrome = useSmabar((s) => s.appearance.barChrome);
  const onAuxClick =
    onMiddleClick === undefined
      ? undefined
      : (e: MouseEvent<HTMLDivElement>) => {
          if (e.button === 1) onMiddleClick();
        };
  return (
    <div
      className={`surface-bar flex items-center ${fit ? "" : "w-full"}`}
      // Always rounded: --sb-bar-radius decides how much, and 0 is square.
      // Tying it to the width mode meant a full-width bar could not be
      // rounded at all, however much edge margin it was given.
      style={{
        ...rowStyle,
        borderRadius: "var(--sb-bar-radius, 1rem)",
        width: fit ? "fit-content" : undefined,
        maxWidth: fit ? "100%" : undefined,
        marginInline: fit ? "auto" : undefined,
        // Opacity as well: a descendant that sets its own `visibility:
        // visible` (the kit's animated labels do) would show through a
        // merely hidden row; nothing overrides an ancestor's opacity.
        visibility: hidden ? "hidden" : undefined,
        opacity: hidden ? 0 : undefined,
        pointerEvents: hidden ? "none" : undefined,
      }}
      data-bar-root
      data-capture={hidden ? undefined : captureTarget}
      data-bar-hidden={hidden ? "" : undefined}
      data-bar-chrome={barChrome}
      data-input-region={hidden ? undefined : ""}
      inert={hidden}
      aria-hidden={hidden ? true : undefined}
      onAuxClick={onAuxClick}
    >
      {children}
    </div>
  );
}

function OverlayTrigger({ zone }: { zone: ZoneKind }) {
  const open = useSmabar((s) => s.overlayOpen);
  const card = useSmabar((s) => s.appearance.tileChrome === "card");
  return (
    <button
      className={`${card ? "surface-tile surface-tile-hover" : ""} flex shrink-0 items-center justify-center rounded-sb-s p-1.5 ${
        open
          ? "surface-tile-active text-foreground"
          : "bar-hover-foreground text-dim"
      }`}
      data-tile-chrome={card ? "card" : "flat"}
      onClick={(e) => {
        e.stopPropagation();
        setOverlayRowOpen(!open);
      }}
      aria-label={t(
        zone === "plugins"
          ? "bar.overlay.showPlugins"
          : "bar.overlay.showShortcuts",
      )}
      aria-expanded={open}
    >
      <Layers size="1em" />
    </button>
  );
}

/** Outer style carrying the effective row height for every bar row. */
type ShellStyle = CSSProperties &
  Record<"--sb-bar-row-height", string> &
  Partial<Record<"--sb-autohide-edge-gap", string>>;

/**
 * The bar strip: glued to the configured screen edge, one or two zone rows
 * depending on the layout variant, full-width or as a centered
 * content-sized dock (layout.width auto + margin).
 *
 * - `split`: one row, shortcut zone | divider | tile zone.
 * - `rows`:  two stacked rows; the primary zone's row sits at the screen edge.
 * - `solo`:  only the primary zone; the other opens as an overlay row
 *            (trigger button at the right end, or middle-click on the row).
 */
export function BarShell() {
  const layout = useSmabar((s) => s.layout);
  const shortcuts = useSmabar((s) => s.shortcuts);
  const cardTiles = useSmabar((s) => s.appearance.tileChrome === "card");
  const shortcutAlign = useSmabar((s) => s.appearance.shortcutAlign);
  const pluginAlign = useSmabar((s) => s.appearance.pluginAlign);
  const overlayOpen = useSmabar((s) => s.overlayOpen);
  const flyoutOpen = useSmabar((s) => s.openFlyout !== null);
  const legalRequired = useSmabar((s) => s.legalRequired);
  const coverHeight = useSmabar((s) => tallestCover(s.coverHeights));
  const surfaceOpen = flyoutOpen || overlayOpen;
  const { revealed, surfaceRef, hotzoneRef, edgeGapRef } = useAutohide(
    layout.behavior,
    surfaceOpen,
  );

  const primary = layout.primaryZone;
  const secondary: ZoneKind = primary === "shortcuts" ? "plugins" : "shortcuts";
  const auto = legalRequired || layout.width === "auto";
  const margin = clampMargin(layout.margin);
  const autoHide = layout.behavior === "autohide";

  // At auto width the zones size by content: their usual flex-1 (basis 0)
  // would corrupt the content-sized dock's intrinsic width and squeeze a
  // zone into scrolling. Shrink stays allowed for the max-width cap.
  const zoneStyle: CSSProperties | undefined = auto
    ? { flex: "0 1 auto" }
    : undefined;

  // Rows variant: at auto width a content-sized zone is the row's only flex
  // child and would hug the start — auto margins place it after the zone's
  // own alignment setting (data-zone-align only aligns tiles INSIDE a zone).
  const rowsZoneStyle = (kind: ZoneKind): CSSProperties | undefined => {
    if (!auto) return zoneStyle;
    const align = kind === "shortcuts" ? shortcutAlign : pluginAlign;
    if (align === "left") return { ...zoneStyle, marginInlineEnd: "auto" };
    if (align === "right") return { ...zoneStyle, marginInlineStart: "auto" };
    return { ...zoneStyle, marginInline: "auto" };
  };

  let rows: ReactNode;
  if (legalRequired) {
    // First start: until the terms are accepted the bar offers nothing but
    // the way to them. A BarRow, so the tile has an input region under X11.
    rows = (
      <BarRow fit={auto}>
        <LegalGateTile />
      </BarRow>
    );
  } else if (layout.variant === "split") {
    const ratio = clampDividerRatio(layout.dividerRatio);
    // The divider ratio is a percentage flex basis, which only works at full
    // width (percent of a content-sized dock would be circular). At auto both
    // zones are content-sized and the divider is inert (ZoneDivider).
    // ponytail: once an auto dock reaches the screen edge both zones shrink
    // in proportion to their content and scroll; allocating that space by
    // hand exists at full width only.
    rows = (
      <BarRow>
        <ShortcutZone
          style={
            auto ? zoneStyle : { flex: `0 0 ${(ratio * 100).toFixed(2)}%` }
          }
        />
        <ZoneDivider />
        <PluginZone style={zoneStyle} />
        <SettingsButton />
      </BarRow>
    );
  } else if (layout.variant === "rows") {
    // The primary row sits at the screen edge: first row when the bar hangs
    // from the top, last row when it rests on the bottom. The settings gear
    // lives in the lower row (the bar's visual end).
    const first = layout.position === "top" ? primary : secondary;
    const second = layout.position === "top" ? secondary : primary;
    rows = (
      <>
        <BarRow>
          <ZoneContent kind={first} style={rowsZoneStyle(first)} />
        </BarRow>
        <BarRow>
          <ZoneContent kind={second} style={rowsZoneStyle(second)} />
          <SettingsButton />
        </BarRow>
      </>
    );
  } else {
    // Solo: the rows layout with the secondary row shown on demand. It is a
    // bar row like any other — same window, width, centring and input shape
    // — so flyouts from it behave exactly as in the rows variant.
    // The primary row carries the layer toggle before its zone and the gear
    // after it. A three-column grid with equal flexible outer columns keeps
    // the zone on the row's centre line whatever the buttons weigh; left and
    // right alignment give the zone the free space on one side instead.
    // Alone, the visible row hugs its content (the hidden row keeps the dock
    // at the wider width so the window stays put); with both shown they
    // share the dock's width like the rows variant.
    const primaryRow = (
      <BarRow
        fit={auto && !overlayOpen}
        onMiddleClick={() => {
          setOverlayRowOpen(!overlayOpen);
        }}
      >
        <div
          className="bar-solo-row"
          data-align={primary === "shortcuts" ? shortcutAlign : pluginAlign}
          data-auto={auto ? "" : undefined}
        >
          <OverlayTrigger zone={secondary} />
          {/* One grid child whatever the zone renders (a zone may be more
              than one element), so the gear stays in the third column. */}
          <div className="bar-solo-zone">
            <ZoneContent kind={primary} style={zoneStyle} />
          </div>
          <SettingsButton />
        </div>
      </BarRow>
    );
    // Always rendered, hidden while closed: toggling swaps visibility only,
    // so the bar window keeps its size and the row appears with its live
    // content instead of growing in with a stale frame.
    const secondaryRow = (
      <BarRow hidden={!overlayOpen} captureTarget="overlay">
        <ZoneContent kind={secondary} style={rowsZoneStyle(secondary)} />
      </BarRow>
    );
    rows =
      layout.position === "top" ? (
        <>
          {primaryRow}
          {secondaryRow}
        </>
      ) : (
        <>
          {secondaryRow}
          {primaryRow}
        </>
      );
  }

  const shellStyle: ShellStyle = {
    zIndex: BAR,
    "--sb-bar-row-height": coverRowHeightExpr(
      rowHeightExpr(shortcuts, cardTiles),
      coverHeight,
    ),
  };
  // On the shell root so BOTH the sliding content and the hotzone read it.
  if (autoHide) {
    shellStyle["--sb-autohide-edge-gap"] = `${String(margin)}px`;
  }

  // The dock wrapper floats centered and keeps the edge margin on every
  // side, in BOTH width modes — only how it claims horizontal space differs:
  // auto sizes to its content, full takes what the margins leave. The
  // explicit cap applies to full alone (auto is content-sized already), and
  // the margin cap always wins so the bar cannot outgrow its own inset.
  // max-content, NOT fit-content: the native window follows the dock's
  // measured width (inputShape.ts), so the viewport is exactly as wide as
  // the dock — and fit-content is capped at the viewport. Content that grew
  // after the last measurement (tiles registering after start, a zone
  // getting its room back) could then never widen the dock, and the window
  // stayed narrow with the last tile cut off. max-content ignores the
  // containing block; the dock overflows the window for one frame, the
  // geometry report grows the window, and the auto margins re-center it.
  // data-bar-root stays on the rows INSIDE the wrapper, so strut and input
  // shape follow the dock surface, not the screen.
  const edgeCap = `calc(var(--sb-work-area-width, 100vw) - ${String(2 * margin)}px)`;
  const cap = auto ? 0 : clampMaxWidth(layout.maxWidth);
  const dockStyle: CSSProperties = {
    width: auto ? "max-content" : "100%",
    maxWidth: cap === 0 ? edgeCap : `min(${String(cap)}px, ${edgeCap})`,
    marginInline: "auto",
    ...(layout.position === "top"
      ? { marginTop: margin }
      : { marginBottom: margin }),
  };

  return (
    <div
      // No click handler here: an open flyout's fullscreen catcher (which
      // stacks ABOVE the bar) owns every outside click. A bar-level close
      // handler would also catch clicks bubbling out of the portaled flyout
      // through the React tree and close it mid-interaction.
      // select-none: a bar is chrome — a press must never start a text/icon
      // selection (WebKit turns a dragged selection into a native drag that
      // breaks pointer tracking). Flyouts/settings portal out and stay
      // selectable.
      className={`fixed right-0 left-0 flex flex-col select-none ${
        autoHide ? "bar-autohide-shell" : ""
      } ${layout.position === "top" ? "top-0" : "bottom-0"}`}
      style={shellStyle}
      data-autohide={autoHide ? (revealed ? "revealed" : "hidden") : undefined}
      data-bar-position={autoHide ? layout.position : undefined}
    >
      <div
        ref={surfaceRef}
        className={autoHide ? "bar-autohide-surface" : undefined}
        style={dockStyle}
        data-bar-dock
        data-autohide-surface={autoHide ? "" : undefined}
        data-input-region={autoHide ? "" : undefined}
      >
        <div
          // bar-rows: flex column with the themed gap, so the rows variant
          // reads as two separate pills instead of two flush surfaces.
          className="bar-rows"
        >
          {rows}
        </div>
      </div>
      {autoHide && (
        // These regions move with the native window. Swapping them at reveal
        // would drop the pointer before the compositor has moved the window.
        <>
          <div
            ref={hotzoneRef}
            className="bar-autohide-hotzone"
            data-autohide-hotzone
            data-autohide-region="activation"
            data-bar-position={layout.position}
            data-input-region
          />
          <div
            ref={edgeGapRef}
            className="bar-autohide-hotzone"
            data-autohide-gap
            data-autohide-region="edge"
            data-bar-position={layout.position}
            data-input-region
          />
        </>
      )}
    </div>
  );
}
