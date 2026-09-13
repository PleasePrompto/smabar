import { X } from "lucide-react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";

import { t } from "../i18n/t";
import { reportError } from "../ipc/log";
import { reportPopup } from "../ipc/managedPopups";
import { stageNotificationUpdate } from "../ipc/surface";
import { useSmabar } from "../store/bar";
import { brandingFor } from "../plugins/branding";
import { ShadowHost } from "../plugins/PluginContent";
import { popupVertical, type PopupItem } from "../plugins/popupQueue";

/**
 * The proactive popup stack: every visible toast renders near the
 * configured screen position (popups.position), newest closest to the
 * edge; the stack dodges the bar when both share the same edge. Toasts
 * are individually interactive (data-input-region) while the column
 * itself stays click-through.
 */
export function PluginPopup({ children }: { children?: ReactNode }) {
  const visible = useSmabar((state) => state.popupQueue.visible);
  const position = useSmabar((state) => state.popups.position);
  const vertical = popupVertical(position);

  if (visible.length === 0 && !children) return null;

  const style: CSSProperties = {
    // rem so notifications follow the global size slider like everything else.
    width:
      "min(22.5rem, calc(var(--sb-work-area-width, 100vw) - 2 * var(--sb-space-l)))",
    display: "flex",
    // Column order keeps the DOM stable while the NEWEST toast (last in
    // `visible`) always sits closest to the configured edge.
    flexDirection: vertical === "top" ? "column-reverse" : "column",
    gap: "var(--sb-space-s)",
    pointerEvents: "none",
  };

  return (
    <div style={style}>
      {visible.map((item) => (
        <PopupToast key={item.id} item={item} />
      ))}
      {children}
    </div>
  );
}

/**
 * One toast: sticky (ttlMs null) until its X is clicked, otherwise
 * auto-dismissed after its active lifetime. Hover and keyboard focus pause
 * that lifetime; a neighbor's exit re-renders the toast without resetting it.
 */
function PopupToast({ item }: { item: PopupItem }) {
  const dismiss = useSmabar((state) => state.dismissPopup);
  const dismissing = useRef(false);
  const remainingMs = useRef(0);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const dismissAfterHide = useCallback(
    (reason: "dismissed" | "expired" = "dismissed") => {
      if (dismissing.current) return;
      dismissing.current = true;
      void stageNotificationUpdate()
        .then(() => reportPopup(item, reason))
        .then(() => {
          dismiss(item.id);
        })
        .catch((error: unknown) => {
          dismissing.current = false;
          reportError(error);
        });
    },
    [dismiss, item],
  );

  useEffect(() => {
    remainingMs.current =
      item.ttlMs === null
        ? 0
        : Math.max(0, item.shownAtMs + item.ttlMs - Date.now());
  }, [item.shownAtMs, item.ttlMs]);

  useEffect(() => {
    if (item.ttlMs === null || hovered || focused) return;
    const startedAt = Date.now();
    const timer = window.setTimeout(() => {
      dismissAfterHide("expired");
    }, remainingMs.current);
    return () => {
      window.clearTimeout(timer);
      remainingMs.current = Math.max(
        0,
        remainingMs.current - (Date.now() - startedAt),
      );
    };
  }, [dismissAfterHide, focused, hovered, item.ttlMs]);

  return (
    <div
      className="sb-root surface-flyout relative rounded-[var(--sb-radius-l)] p-3 text-[color:var(--sb-text)]"
      style={{ pointerEvents: "auto" }}
      data-input-region
      data-capture="popup"
      role="status"
      aria-live="polite"
      onPointerEnter={() => {
        setHovered(true);
      }}
      onPointerLeave={() => {
        setHovered(false);
      }}
      onFocusCapture={() => {
        setFocused(true);
      }}
      onBlurCapture={(event) => {
        if (
          !(event.relatedTarget instanceof Node) ||
          !event.currentTarget.contains(event.relatedTarget)
        ) {
          setFocused(false);
        }
      }}
      onClick={(event) => {
        event.stopPropagation();
      }}
      onAuxClick={(event) => {
        event.stopPropagation();
      }}
    >
      <button
        type="button"
        className="notification-close sb-btn sb-btn-ghost sb-btn-icon absolute top-2 right-2 z-10"
        aria-label={t("popup.dismiss")}
        title={t("popup.dismiss")}
        onClick={() => {
          dismissAfterHide();
        }}
      >
        <X size="1.125rem" />
      </button>
      <div
        className="sb-scroll overflow-y-auto pr-7"
        style={{
          maxHeight:
            "min(25rem, calc(var(--sb-work-area-height) - 2 * var(--sb-space-xl)))",
        }}
      >
        <ShadowHost
          pluginId={item.pluginId}
          tileId={item.tileId}
          html={item.html}
          target="popup"
          memoryScope={String(item.id)}
          popupInstanceId={item.instanceId}
          style={brandingFor(item.pluginId, item.tileId)}
        />
      </div>
    </div>
  );
}
