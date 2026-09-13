import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { closeCurrentSurface, reportNotificationMeasure } from "../ipc/surface";
import { cleanupListeners } from "../ipc/listeners";
import { reportError } from "../ipc/log";
import { useSmabar } from "../store/bar";
import { PluginPopup } from "./PluginPopup";
import { Toast } from "./Toast";
import { AppUpdateNotification } from "./AppUpdateNotification";
import { showUpdateNotification } from "../ipc/updateSync";
import { cssLength } from "../theme/cssLength";

interface Point {
  x: number;
  y: number;
}

interface Placement {
  popup?: Point | null;
  notice?: Point | null;
}

export function NotificationSurface() {
  const visible = useSmabar((state) => state.popupQueue.visible);
  const updateVisible = useSmabar(showUpdateNotification);
  const updateStatus = useSmabar((state) => state.updateStatus);
  const updateOffer = useSmabar((state) => state.updateOffer);
  const updating =
    updateStatus.state === "downloading" || updateStatus.state === "installing";
  const hasPopups = visible.length > 0 || updateVisible;
  const notice = useSmabar((state) => state.notice);
  const popupPosition = useSmabar((state) => state.popups.position);
  const barPosition = useSmabar((state) => state.layout.position);
  const popupRef = useRef<HTMLDivElement>(null);
  const noticeRef = useRef<HTMLDivElement>(null);
  const hadContent = useRef(false);
  const [placement, setPlacement] = useState<Placement>({});

  useEffect(() => {
    return cleanupListeners([
      listen<Placement>("notification-placement", (event) => {
        setPlacement(event.payload);
      }),
    ]);
  }, []);

  useLayoutEffect(() => {
    const popup = popupRef.current;
    const shellNotice = noticeRef.current;
    const report = () => {
      void reportNotificationMeasure({
        popup:
          popup === null
            ? null
            : { width: popup.offsetWidth, height: popup.offsetHeight },
        notice:
          shellNotice === null
            ? null
            : {
                width: shellNotice.offsetWidth,
                height: shellNotice.offsetHeight,
              },
        edgeInset: cssLength("--sb-space-l", 20),
        gap: cssLength("--sb-space-s", 12),
      }).catch(reportError);
    };
    report();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(report);
    if (popup !== null) observer.observe(popup);
    if (shellNotice !== null) observer.observe(shellNotice);
    return () => {
      observer.disconnect();
    };
  }, [
    barPosition,
    notice,
    popupPosition,
    visible,
    updateVisible,
    updateStatus,
    updateOffer,
  ]);

  useEffect(() => {
    if (hasPopups || notice !== null) {
      hadContent.current = true;
      return;
    }
    // An empty measure hides the native window during installation. Keep its
    // listener alive so a failed download can show the retry notification.
    if (hadContent.current && !updating)
      void closeCurrentSurface().catch(reportError);
  }, [notice, hasPopups, updating]);

  return (
    <div className="relative size-full">
      {hasPopups && (
        <div
          ref={popupRef}
          className="absolute"
          style={{
            left: placement.popup?.x ?? 0,
            top: placement.popup?.y ?? 0,
          }}
        >
          <PluginPopup>
            {updateVisible && <AppUpdateNotification />}
          </PluginPopup>
        </div>
      )}
      {notice !== null && (
        <div
          ref={noticeRef}
          className="absolute"
          style={{
            left: placement.notice?.x ?? 0,
            top: placement.notice?.y ?? 0,
          }}
        >
          <Toast />
        </div>
      )}
    </div>
  );
}
