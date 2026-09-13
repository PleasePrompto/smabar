import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { getTile } from "./registry";
import { flyoutContentFor } from "./overlay/model";
import { t } from "../i18n/t";
import { cleanupListeners } from "../ipc/listeners";
import { reportError } from "../ipc/log";
import {
  recordMemoryBatch,
  recordMemoryUi,
  suppressMemoryStateUpdate,
} from "../ipc/memoryProbe";
import { createPluginUiPull } from "../ipc/pluginUiPull";
import {
  closeFlyoutSurface,
  pinFlyoutSurface,
  reportFlyoutMeasure,
  reportOverlayPointer,
  type FlyoutPlacement,
  type FlyoutRequest,
} from "../ipc/overlay";
import { useSmabar } from "../store/bar";
import { brandingStyle } from "../plugins/branding";
import { SUPPRESS_DELEGATED_CLICK_ATTR } from "../plugins/behaviour/delegate";
import { ShadowHost } from "../plugins/PluginContent";
import { cssLength } from "../theme/cssLength";

const CLOSE_DELAY_MS = 160;

/** Both the bar click and a click on the preview use the overlay's live UI. */
function pinPreview(request: FlyoutRequest): void {
  const definition = getTile(request.tileId);
  if (definition === undefined || request.mode !== "peek") return;
  const key = `${definition.pluginId}/${definition.tile.id}`;
  const ui = useSmabar.getState().pluginUi;
  const preview = flyoutContentFor(
    "peek",
    ui[`${key}/hover`],
    ui[`${key}/flyout`],
  );
  const pinned = flyoutContentFor(
    "pinned",
    ui[`${key}/hover`],
    ui[`${key}/flyout`],
  );
  // Remote players are absent from previews, even when the raw HTML matches.
  const replaceContent = preview !== pinned || hasEmbed(pinned);
  void pinFlyoutSurface(request.generation, replaceContent).catch(reportError);
}

function hasEmbed(html: string | undefined): boolean {
  return html?.toLowerCase().includes("<iframe") === true;
}

export function OverlaySurface({ onReady }: { onReady?: () => void }) {
  const registryVersion = useSmabar((state) => state.registryVersion);
  const [request, setRequest] = useState<FlyoutRequest | null>(null);
  const [placement, setPlacement] = useState<FlyoutPlacement | null>(null);
  const [closing, setClosing] = useState(false);
  const [contentSettled, setContentSettled] = useState(true);
  const rootRef = useRef<HTMLDivElement>(null);
  const flyoutRef = useRef<HTMLDivElement>(null);
  // A closed generation remains as a tombstone: a staged open can arrive late.
  const requestRef = useRef<
    FlyoutRequest | { generation: number; mode: null } | null
  >(null);
  const contentFrame = useRef(0);
  // The last geometry handed to the core. A plugin re-rendering its open
  // flyout every second must not re-run the native place/reveal chain (a
  // WebKit snapshot plus forced paints) when nothing about the box changed.
  const lastMeasure = useRef("");
  const definition = request === null ? undefined : getTile(request.tileId);
  const uiKey =
    definition === undefined
      ? null
      : `${definition.pluginId}/${definition.tile.id}`;
  const hoverHtml = useSmabar((state) =>
    uiKey === null ? undefined : state.pluginUi[`${uiKey}/hover`],
  );
  const flyoutHtml = useSmabar((state) =>
    uiKey === null ? undefined : state.pluginUi[`${uiKey}/flyout`],
  );
  const html = flyoutContentFor(request?.mode ?? null, hoverHtml, flyoutHtml);

  useEffect(() => {
    let disposed = false;
    // The pull returns only renders of the open flyout's generation; the
    // filter guards against a switch between signal and response.
    const pull = createPluginUiPull((rendered) => {
      recordMemoryBatch();
      const current = requestRef.current;
      for (const render of rendered) {
        recordMemoryUi(render.html.length, "live");
        if (suppressMemoryStateUpdate()) continue;
        if (
          current?.mode == null ||
          current.generation !== render.generation ||
          current.tileId !== `plugin:${render.pluginId}:${render.tileId}` ||
          (render.target !== "hover" && render.target !== "flyout")
        )
          continue;
        useSmabar
          .getState()
          .setPluginUi(
            `${render.pluginId}/${render.tileId}/${render.target}`,
            render.html,
          );
      }
    });
    const registrations = [
      listen<{ generation: number }>("plugin-ui-overlay", ({ payload }) => {
        const current = requestRef.current;
        if (current?.mode != null && current.generation === payload.generation)
          pull();
      }),
      listen<number>("flyout-pin-requested", (event) => {
        const current = requestRef.current;
        if (current?.generation === event.payload && current.mode !== null)
          pinPreview(current);
      }),
      listen<FlyoutRequest>("surface-flyout", (event) => {
        const { content, ...nextRequest } = event.payload;
        for (const html of Object.values(content ?? {})) {
          if (html !== null) recordMemoryUi(html.length, "snapshot");
        }
        const current = requestRef.current;
        // Rapid flyout switches run their staging concurrently in the core,
        // so an older request can be delivered after its replacement.
        if (
          current !== null &&
          (nextRequest.generation < current.generation ||
            (nextRequest.generation === current.generation &&
              (current.mode === null || current.tileId !== nextRequest.tileId)))
        )
          return;
        if (content !== undefined) {
          const definition = getTile(nextRequest.tileId);
          const pluginUi: Record<string, string> = {};
          if (definition !== undefined) {
            const key = `${definition.pluginId}/${definition.tile.id}`;
            for (const target of ["hover", "flyout"] as const) {
              const html = content[target];
              if (html !== null) pluginUi[`${key}/${target}`] = html;
            }
          }
          useSmabar.setState({ pluginUi });
        }
        const upgrading =
          current?.generation === nextRequest.generation &&
          current.mode === "peek" &&
          nextRequest.mode === "pinned";
        const replacing = upgrading && nextRequest.preserveContent !== true;
        // Only staged replacements need another native place/reveal chain.
        if (!upgrading || replacing) lastMeasure.current = "";
        requestRef.current = nextRequest;
        window.cancelAnimationFrame(contentFrame.current);
        setContentSettled(!replacing);
        if (replacing) {
          contentFrame.current = window.requestAnimationFrame(() => {
            setContentSettled(true);
          });
        }
        setRequest(nextRequest);
        setPlacement((current) =>
          current?.generation === nextRequest.generation ? current : null,
        );
        setClosing(false);
      }),
      listen<FlyoutPlacement>("overlay-placement", (event) => {
        const current = requestRef.current;
        if (
          current?.generation === event.payload.generation &&
          current.mode !== null
        )
          setPlacement(event.payload);
      }),
      listen<FlyoutRequest>("flyout-closed", (event) => {
        const current = requestRef.current;
        if (current !== null && event.payload.generation < current.generation)
          return;
        requestRef.current = {
          generation: event.payload.generation,
          mode: null,
        };
        useSmabar.setState({ pluginUi: {} });
        setRequest(null);
        setPlacement(null);
      }),
    ];
    const cleanup = cleanupListeners(registrations);
    void Promise.all(registrations)
      .then(() => {
        if (!disposed) onReady?.();
      })
      .catch(reportError);
    return () => {
      disposed = true;
      window.cancelAnimationFrame(contentFrame.current);
      cleanup();
    };
  }, [onReady]);

  useLayoutEffect(() => {
    const element = rootRef.current;
    if (element === null || request === null) return;
    const report = () => {
      const measure = {
        generation: request.generation,
        width: element.offsetWidth,
        height: element.offsetHeight,
        inset: cssLength("--sb-space-m", 16),
        // The core measures the gap from the TILE; adding the row padding
        // puts the flyout body one --sb-space-m off the bar's edge, above
        // and below alike.
        gap: cssLength("--sb-space-m", 8) + cssLength("--sb-bar-pad-y", 6),
        pointerReserve: 0,
      };
      const key = JSON.stringify(measure);
      if (key === lastMeasure.current) return;
      lastMeasure.current = key;
      void reportFlyoutMeasure(measure).catch(reportError);
    };
    report();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(report);
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [html, registryVersion, request]);

  useEffect(() => {
    if (request === null) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setClosing(true);
      window.setTimeout(() => {
        void closeFlyoutSurface(request.generation).catch(reportError);
      }, CLOSE_DELAY_MS);
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [request]);

  // A preview is looked at, not operated: the pointer still reaches its
  // content (tooltips, hover styles), but the first click pins the flyout
  // instead of acting — no plugin action, no link, no focus. `inert` used to
  // do this and swallowed every hover with it, so a title inside a preview
  // never showed a tooltip. Native capture listeners: they run before the
  // plugin content's own handlers even for clicks inside its shadow root.
  useEffect(() => {
    const element = flyoutRef.current;
    if (element === null || request?.mode !== "peek") return;
    const keepUnfocused = (event: Event) => {
      event.preventDefault();
    };
    const pinInstead = (event: Event) => {
      event.preventDefault();
      event.stopPropagation();
      pinPreview(request);
    };
    element.addEventListener("mousedown", keepUnfocused, true);
    element.addEventListener("click", pinInstead, true);
    return () => {
      element.removeEventListener("mousedown", keepUnfocused, true);
      element.removeEventListener("click", pinInstead, true);
    };
  }, [request]);

  if (request === null) return null;
  const source = html === flyoutHtml ? "flyout" : "hover";
  const { direction = "up", pointerX = 0, x = 0, y = 0 } = placement ?? {};
  const peek = request.mode === "peek";
  const originY = direction === "down" ? "0" : "100%";

  return (
    <div
      ref={rootRef}
      className="relative"
      style={{
        position: "absolute",
        left: x,
        top: y,
        width:
          "min(21.25rem, calc(var(--sb-work-area-width) - 2 * var(--sb-space-m)))",
        visibility:
          placement?.generation === request.generation ? "visible" : "hidden",
      }}
      onPointerEnter={() => {
        void reportOverlayPointer(true).catch(reportError);
      }}
      onPointerLeave={() => {
        void reportOverlayPointer(false).catch(reportError);
      }}
    >
      <div className="overlay-token-scope" data-direction={direction}>
        <div
          className="surface-flyout relative rounded-[var(--sb-radius-l,0.875rem)] text-[color:var(--sb-text,#fff)]"
          style={{
            // Grows out of the tile it belongs to.
            transformOrigin: `${String(pointerX)}px ${originY}`,
            animationName: closing ? "flyoutOut" : "flyoutY",
            animationDuration: closing
              ? `${String(CLOSE_DELAY_MS)}ms`
              : "var(--sb-dur-normal)",
            animationTimingFunction: "var(--sb-ease-out)",
            animationFillMode: "both",
          }}
          data-capture="flyout"
          ref={flyoutRef}
          onClick={(event) => {
            event.stopPropagation();
          }}
          onAuxClick={(event) => {
            event.stopPropagation();
          }}
        >
          <div
            className="surface-scroll flyout-content-transition overflow-y-auto p-4"
            style={{
              maxHeight: "calc(var(--sb-work-area-height) * 0.78)",
            }}
            data-settled={contentSettled ? "" : undefined}
            {...(peek ? { [SUPPRESS_DELEGATED_CLICK_ATTR]: "" } : {})}
          >
            {definition !== undefined && html !== undefined ? (
              <ShadowHost
                key={definition.id}
                pluginId={definition.pluginId}
                tileId={definition.tile.id}
                tileKey={definition.id}
                html={html}
                memoryScope={source}
                probeGeneration={request.generation}
                allowEmbeds={request.mode === "pinned" && hasEmbed(html)}
                style={brandingStyle(definition.tile)}
              />
            ) : (
              <p className="text-xs text-faint">{t("plugin.noContent")}</p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
