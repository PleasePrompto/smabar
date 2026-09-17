import { useCallback, useLayoutEffect, useRef } from "react";

import { reportError } from "../../ipc/log";
import { reportFlyoutMeasure, type FlyoutRequest } from "../../ipc/overlay";
import { DEFAULT_FLYOUT_WIDTH } from "../../plugins/flyoutWidth";
import { cssLength } from "../../theme/cssLength";
import type { PluginTileDefinition } from "../registry";

/** Apply sanitized width requests before measuring; unchanged boxes never
 * restart the native place/reveal chain (WebKit snapshot or Windows hide/show). */
export function useFlyoutMeasure(
  request: FlyoutRequest | null,
  html: string | undefined,
  definition: PluginTileDefinition | undefined,
) {
  const rootRef = useRef<HTMLDivElement>(null);
  const preferredWidth = useRef(DEFAULT_FLYOUT_WIDTH);
  const lastMeasure = useRef("");
  const resetMeasure = useCallback(() => {
    lastMeasure.current = "";
  }, []);
  const applyWidth = useCallback((width: string) => {
    preferredWidth.current = width;
    if (rootRef.current !== null && rootRef.current.style.width !== width)
      rootRef.current.style.width = width;
  }, []);

  useLayoutEffect(() => {
    const element = rootRef.current;
    if (element === null || request === null) return;
    const report = () => {
      // Child layout effects can run before the parent's ref is attached.
      applyWidth(
        html === undefined || definition === undefined
          ? DEFAULT_FLYOUT_WIDTH
          : preferredWidth.current,
      );
      const measure = {
        generation: request.generation,
        width: element.offsetWidth,
        height: element.offsetHeight,
        inset: cssLength("--sb-space-m", 16),
        // Core measures from the tile; include row padding for the bar gap.
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
  }, [html, definition, request, applyWidth]);

  return { rootRef, applyWidth, resetMeasure };
}
