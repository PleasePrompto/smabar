import { TriangleAlert } from "lucide-react";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  type MouseEvent,
  type RefObject,
} from "react";

import { useFlyoutTrigger } from "../components/useFlyoutTrigger";
import { PluginTile } from "../components/PluginTile";
import { t } from "../i18n/t";
import { call } from "../ipc/call";
import { reportError } from "../ipc/log";
import {
  memoryProbeIs,
  recordMemoryClockStarts,
  recordMemoryDomCommit,
  useMemoryProbeHtml,
} from "../ipc/memoryProbe";
import { useSmabar, type TileChrome } from "../store/bar";
import { brandingStyle, type BrandingStyle } from "./branding";
import { captureChartMemory, clearChartMemory, enhanceCharts } from "./charts";
import { enhanceClocks } from "./clock";
import {
  enhanceBadges,
  enhanceCarousels,
  enhanceMarquees,
  enhanceRotators,
  enhanceTabs,
  enhanceTooltips,
} from "./decorators";
import { enhanceIcons } from "./icons";
import { syncKit } from "./behaviour";
import { clearTweenMemory } from "./behaviour/tween";
import { adoptKit } from "./kitSheet";
import { resolveAsset } from "./assets";
import { resolveEmbed } from "./embeds";
import { reportMarkupDrops, reportUnknownKitClasses } from "./markupReport";
import { reportMarkupLint } from "./markupLint";
import { sanitizeHtml, type SanitizerDrop } from "./sanitize";
import {
  collectFieldValues,
  rangeActionIntent,
  restoreFields,
  snapshotFields,
} from "./fields";
import { PluginIcon } from "./PluginIcon";
import {
  actionElementInPath,
  handlesItsOwnClick,
  sendPluginAction,
  submitIntent,
} from "./pluginActions";
import { useRangeActions } from "./rangeActions";
import type { PluginTileDefinition } from "../components/registry";

export {
  collectFieldValues,
  restoreFields,
  snapshotFields,
  type FieldSnapshot,
} from "./fields";
export {
  actionElementInPath,
  handlesItsOwnClick,
  sendPluginAction,
  submitIntent,
} from "./pluginActions";

/** Tile entry of a "plugin-added" event payload. */
export interface PluginTileDef {
  id: string;
  name: string;
  hasFlyout?: boolean;
  /** Optional tile chrome override; absent follows the global appearance. */
  tile?: TileChrome;
  /** Base text size of a two-line tile (`sb-tile-stack`); absent = "m". */
  tileScale?: "s" | "m" | "l";
  /** Per-tile branding (validated CSS color values from the manifest). */
  accent?: string;
  accent2?: string;
  accentFg?: string;
  /** Optional sanitized SVG prepended to the tile as its branding icon. */
  iconSvg?: string;
  /** Opt in to the folder icon in the cover; a usable iconSvg wins. */
  usePluginIcon?: boolean;
}

/** Payload of the core's "plugin-added" event. */
export interface PluginAddedPayload {
  pluginId: string;
  name: string;
  /** Normalized folder icon for settings and covers that opt in. */
  iconDataUrl?: string | null;
  tiles: PluginTileDef[];
  /** Raw JSON Schema from the manifest; absent when none is declared. */
  settingsSchema?: unknown;
}

interface ShadowHostProps {
  pluginId: string;
  tileId: string;
  html: string;
  /** Registry id of the owning tile; lets a right-click inside plugin
   *  markup fall back to that tile's menu. Popups pass none — they belong
   *  to no registered tile. */
  tileKey?: string;
  /** Surface this HTML renders on. Published as `data-target` on the
   *  `.sb-root` wrapper so the kit can constrain the bar tile, which has a
   *  hard height budget the roomier surfaces do not. */
  target?: "tile" | "flyout" | "popup";
  /** Manifest tile scale; set on the host so it inherits into the shadow
   *  tree as the two-line tile's size multipliers. */
  tileScale?: "s" | "m" | "l";
  /** Remote players need the pinned flyout's full-window input region. */
  allowEmbeds?: boolean;
  /** Distinguishes concurrent shell-owned instances of the same surface. */
  memoryScope?: string;
  /** Managed toast identity; core rejects actions after its session ends. */
  popupInstanceId?: number;
  className?: string;
  style?: BrandingStyle;
}

/**
 * Renders sanitized plugin HTML in an open shadow root, wrapped in a
 * shell-owned `.sb-root` div so the UI kit's element base rules apply; the
 * kit stylesheet itself is adopted via {@link adoptKit}. Icons and charts
 * are enhanced post-sanitize. Clicks on [data-action] elements become
 * plugin_action invokes and stop there — anything else bubbles on (e.g. to
 * the tile's flyout toggle).
 */
export function ShadowHost({
  pluginId,
  tileId,
  html,
  tileKey,
  target = "flyout",
  tileScale,
  allowEmbeds = false,
  memoryScope,
  popupInstanceId,
  className,
  style,
}: ShadowHostProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const probeHtml = useMemoryProbeHtml(html);
  const {
    renderedHtml,
    applyPending: applyPendingRange,
    preserveHovered: preserveHoveredRange,
    bind: bindRangeActions,
  } = useRangeActions(probeHtml, pluginId, tileId, popupInstanceId);
  const memoryKey = `${pluginId}/${tileId}/${target}${memoryScope === undefined ? "" : `/${memoryScope}`}`;
  const renderedMemoryKey = useRef<string | undefined>(undefined);

  useEffect(
    () => () => {
      clearChartMemory(memoryKey);
      clearTweenMemory(memoryKey);
    },
    [memoryKey],
  );
  useEffect(() => {
    if (target !== "tile" || tileKey === undefined) return;
    return () => {
      useSmabar.getState().setCoverHeight(tileKey, null);
    };
  }, [target, tileKey]);

  useLayoutEffect(() => {
    const host = hostRef.current;
    if (host === null) return;
    // Reuse the root across renders and StrictMode re-runs — a second
    // attachShadow on the same host throws.
    const root = host.shadowRoot ?? host.attachShadow({ mode: "open" });
    // Plugins re-render on every data update; carry typed-in field values
    // and focus across the replace so a poll doesn't eat the user's input.
    const snapshot = snapshotFields(root);
    const wrapper = document.createElement("div");
    wrapper.className = "sb-root";
    wrapper.dataset.target = target;
    // Value-memory scope: charts.ts and behaviour/tween.ts remember the last
    // painted value per tile surface, so a rebuilt DOM can glide from the
    // old value to the new one. Shell-owned, never an author hook.
    wrapper.dataset.sbScope = memoryKey;
    // Collected per render, then reported once per distinct problem set —
    // a plugin rendering every second must not log every second.
    const drops: SanitizerDrop[] = [];
    const markup = sanitizeHtml(
      renderedHtml,
      (value) => resolveAsset(pluginId, value),
      (drop) => drops.push(drop),
      allowEmbeds ? resolveEmbed : null,
    );
    reportMarkupDrops(pluginId, tileId, target, drops);
    reportUnknownKitClasses(pluginId, tileId, target, markup);
    reportMarkupLint(pluginId, tileId, target, markup);
    wrapper.appendChild(markup);
    applyPendingRange(wrapper);
    preserveHoveredRange(wrapper);
    if (renderedMemoryKey.current === memoryKey) {
      captureChartMemory(root, memoryKey);
    }
    root.replaceChildren(wrapper);
    recordMemoryDomCommit(renderedHtml.length);
    renderedMemoryKey.current = memoryKey;
    // After replaceChildren: the <style> fallback must survive the swap.
    adoptKit(root);
    restoreFields(root, snapshot);
    // After restoreFields: filters and steppers derive from the restored
    // values, not from what the plugin just rendered.
    syncKit(wrapper);
    enhanceIcons(wrapper);
    enhanceCharts(wrapper, memoryKey);
    enhanceTooltips(wrapper);
    const stopMarquees = enhanceMarquees(wrapper);
    enhanceBadges(wrapper);
    const stopClocks = memoryProbeIs("no-clocks")
      ? undefined
      : enhanceClocks(wrapper);
    if (stopClocks !== undefined) recordMemoryClockStarts(wrapper);
    const stopCarousels = enhanceCarousels(wrapper);
    const stopRotators = enhanceRotators(wrapper);
    enhanceTabs(wrapper);
    // The cover's natural height, read while the kit's max-height clamps it
    // (`safe center` keeps the overflow measurable). The bar row grows to the
    // tallest cover instead of cutting it off.
    if (target === "tile" && tileKey !== undefined) {
      useSmabar.getState().setCoverHeight(tileKey, wrapper.scrollHeight);
    }
    const stopRangeActions = bindRangeActions(root, wrapper, renderedHtml);
    // Native submit events are not composed: React's host listener cannot
    // see them across the shadow boundary. Keep validation and Enter native.
    const onSubmit = (event: Event) => {
      event.preventDefault();
      event.stopPropagation();
      const intent = submitIntent(
        event.composedPath()[0] ?? null,
        event instanceof SubmitEvent ? event.submitter : null,
      );
      if (intent !== null) {
        sendPluginAction(
          pluginId,
          tileId,
          intent.action,
          intent.value ?? collectFieldValues(root),
          popupInstanceId,
        );
      }
    };
    root.addEventListener("submit", onSubmit, true);
    return () => {
      root.removeEventListener("submit", onSubmit, true);
      stopRangeActions();
      stopMarquees();
      stopClocks?.();
      stopCarousels();
      stopRotators();
    };
  }, [
    renderedHtml,
    target,
    allowEmbeds,
    pluginId,
    tileId,
    tileKey,
    applyPendingRange,
    preserveHoveredRange,
    bindRangeActions,
    memoryKey,
    popupInstanceId,
  ]);

  const onClick = (event: MouseEvent<HTMLDivElement>) => {
    const path = event.nativeEvent.composedPath();
    for (const hop of path) {
      if (hop === event.currentTarget) break; // left the shadow tree
      // The webview must NEVER navigate away from the bar.
      if (hop instanceof HTMLAnchorElement) event.preventDefault();
    }
    const actionElement = actionElementInPath(path, event.currentTarget);
    if (actionElement !== null) {
      event.stopPropagation();
      // Range actions use the committed native change above; sending their
      // click too would duplicate a backend volume change.
      if (rangeActionIntent(actionElement) !== null) return;
      // Its native submit event sends the action after constraint validation.
      if (
        actionElement instanceof HTMLButtonElement &&
        actionElement.form !== null &&
        actionElement.type === "submit"
      )
        return;
      const { action, value } = actionElement.dataset;
      const shadow = hostRef.current?.shadowRoot;
      const payload =
        value ?? (shadow ? collectFieldValues(shadow) : undefined);
      if (action !== undefined) {
        sendPluginAction(pluginId, tileId, action, payload, popupInstanceId);
      }
      return;
    }
    for (const hop of path) {
      if (hop === event.currentTarget) return;
      if (!(hop instanceof HTMLElement)) continue;
      if (handlesItsOwnClick(hop)) {
        event.stopPropagation();
        return;
      }
      const href = hop.getAttribute("href");
      if (hop instanceof HTMLAnchorElement && href !== null) {
        // Sanitized hrefs are http(s); the core validates again and opens
        // the link in the system browser.
        event.stopPropagation();
        void call("open_url", { url: href }).catch(reportError);
        return;
      }
    }
  };

  // Middle clicks on links would also navigate the webview — block them.
  const onAuxClick = (event: MouseEvent<HTMLDivElement>) => {
    for (const hop of event.nativeEvent.composedPath()) {
      if (hop === event.currentTarget) return;
      if (hop instanceof HTMLAnchorElement) {
        event.preventDefault();
        return;
      }
    }
  };

  return (
    <div
      ref={hostRef}
      onClick={onClick}
      onAuxClick={onAuxClick}
      className={className}
      style={style}
      // Markers for the context menu: a right-click inside the shadow tree
      // finds this host through composedPath and knows which plugin tile
      // to send the selected action to (ContextMenuLayer).
      data-plugin-id={pluginId}
      data-plugin-tile={tileId}
      data-tile-id={tileKey}
      data-tile-scale={tileScale}
    />
  );
}

/**
 * Builds the registry component for one plugin tile, with the live zone and
 * flyout contents streamed in as sanitized plugin HTML. Tile actions belong
 * in the flyout header (sb-header) — the bar row stays a calm tile row.
 */
export function PluginContent({
  definition,
}: {
  definition: PluginTileDefinition;
}) {
  const { pluginId, tile } = definition;
  const tileId = tile.id;
  const coverIcon =
    tile.usePluginIcon === true ? definition.iconDataUrl : undefined;
  const flyoutId = `plugin:${pluginId}:${tileId}`;
  const uiKey = `${pluginId}/${tileId}`;
  const branding = brandingStyle(tile);
  const status = useSmabar((s) => s.pluginStatus[pluginId]);
  const tileHtml = useSmabar((s) => s.pluginUi[`${uiKey}/tile`]);
  const hoverHtml = useSmabar((s) => s.pluginUi[`${uiKey}/hover`]);
  const hoverPeekEnabled = useSmabar((s) => s.effects.hoverPeek.enabled);
  // Pushed hover content makes the preview work even when the generic
  // hover-peek effect is globally disabled (explicit plugin override).
  const liveZone = useFlyoutTrigger(flyoutId, {
    forcePeek: hoverHtml !== undefined,
  });
  const failed = status?.status === "failed";
  const peekable = tile.hasFlyout === true || hoverHtml !== undefined;

  const onTile = (
    event: MouseEvent<HTMLButtonElement>,
    ref: RefObject<HTMLButtonElement | null>,
  ) => {
    sendPluginAction(pluginId, tileId, "tile");
    if (tile.hasFlyout === true) liveZone.trigger(event, ref);
  };

  return (
    <div
      // items-stretch: the card (PluginTile) fills the row's content box, so
      // every cover background is exactly as tall as the tallest one; the
      // tile centers its content inside.
      className="relative flex items-stretch gap-1.5"
      data-hover-flyout={
        hoverHtml !== undefined || (hoverPeekEnabled && tile.hasFlyout === true)
          ? ""
          : undefined
      }
    >
      <PluginTile
        triggerRef={peekable ? liveZone.ref : undefined}
        tileId={flyoutId}
        label={tile.name}
        onTrigger={onTile}
        isActive={liveZone.isActive}
        hasFlyout={tile.hasFlyout}
        chrome={tile.tile}
        onPeekEnter={peekable ? liveZone.peekEnter : undefined}
        onPeekLeave={peekable ? liveZone.peekLeave : undefined}
      >
        {(tile.iconSvg !== undefined || coverIcon) && (
          <PluginIcon
            pluginId={pluginId}
            tileId={tileId}
            svg={tile.iconSvg}
            dataUrl={coverIcon}
            className="plugin-icon shrink-0 text-accent"
            style={branding}
          />
        )}
        {failed ? (
          <span
            className="flex items-center gap-1.5"
            data-sb-tooltip={t("plugin.failed")}
          >
            <TriangleAlert size="1em" className="text-danger" />
            <span className="text-muted">{tile.name}</span>
          </span>
        ) : tileHtml === undefined ? (
          <span
            className="flex items-center gap-1.5"
            data-sb-tooltip={t("plugin.loading")}
          >
            <span className="live-dot bg-accent h-1.5 w-1.5 rounded-full" />
            <span className="text-muted">{tile.name}</span>
          </span>
        ) : tileHtml === "" ? null : (
          <ShadowHost
            pluginId={pluginId}
            tileId={tileId}
            tileKey={flyoutId}
            html={tileHtml}
            target="tile"
            tileScale={tile.tileScale}
            style={branding}
          />
        )}
      </PluginTile>
    </div>
  );
}
