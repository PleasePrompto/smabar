import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";

import { reportError } from "../ipc/log";
import { cleanupListeners } from "../ipc/listeners";
import {
  reportTooltipMeasure,
  type TooltipPlacement,
  type TooltipRequest,
} from "../ipc/overlay";
import { TOOLTIP } from "../styles/layers";

export function OverlayTooltip() {
  const [request, setRequest] = useState<TooltipRequest | null>(null);
  const [placement, setPlacement] = useState<TooltipPlacement | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    return cleanupListeners([
      listen<TooltipRequest>("surface-tooltip", (event) => {
        setRequest(event.payload);
        setPlacement((current) =>
          current?.generation === event.payload.generation ? current : null,
        );
      }),
      listen<TooltipPlacement>("tooltip-placement", (event) => {
        setPlacement(event.payload);
      }),
      listen("tooltip-closed", () => {
        setRequest(null);
        setPlacement(null);
      }),
    ]);
  }, []);

  const reportMeasure = useCallback(() => {
    if (request === null || ref.current === null) return;
    const rect = ref.current.getBoundingClientRect();
    void reportTooltipMeasure({
      generation: request.generation,
      width: Math.ceil(rect.width),
      height: Math.ceil(rect.height),
      inset: 8,
      gap: 8,
    }).catch(reportError);
  }, [request]);

  useLayoutEffect(reportMeasure, [reportMeasure]);

  if (request === null) return null;
  return (
    <div
      ref={ref}
      className="overlay-tooltip absolute"
      aria-hidden="true"
      data-side={placement?.side}
      style={{
        zIndex: TOOLTIP,
        left: placement?.x ?? 0,
        top: placement?.y ?? 0,
        visibility:
          placement?.generation === request.generation ? "visible" : "hidden",
      }}
    >
      {request.text}
    </div>
  );
}
