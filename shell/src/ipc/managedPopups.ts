import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSmabar } from "../store/bar";
import {
  clampPopupTtl,
  type PopupItem,
  type PopupRequest,
} from "../plugins/popupQueue";
import { reportError } from "./log";
import { stageNotificationUpdate } from "./surface";

interface ManagedPopup extends PopupRequest {
  instanceId: number;
}

export async function reportPopup(
  item: PopupItem,
  state: string,
): Promise<void> {
  if (item.instanceId === undefined) return;
  await invoke("popup_event_report", { instanceId: item.instanceId, state });
}

/** Snapshot reconciliation makes startup, missed events and upserts recoverable. */
export async function initManagedPopups(): Promise<void> {
  let reconciling = false;
  let refreshing = false;
  let dirty = false;
  const applySnapshot = async () => {
    const popups = await invoke<ManagedPopup[]>("get_managed_popups");
    // A newer event can arrive during the fetch; only check staleness after it resolves.
    if (dirty) return;
    const current = useSmabar.getState().popupQueue;
    const managed = [...current.visible, ...current.queued].filter(
      (item) => item.instanceId !== undefined,
    );
    if (
      managed.length === popups.length &&
      popups.every((popup) =>
        managed.some(
          (item) =>
            item.instanceId === popup.instanceId &&
            item.html === popup.html &&
            item.ttlMs === clampPopupTtl(popup.ttlMs),
        ),
      )
    )
      return;
    await stageNotificationUpdate();
    const wanted = new Set(popups.map((item) => item.instanceId));
    reconciling = true;
    try {
      const store = useSmabar.getState();
      for (const item of [
        ...store.popupQueue.visible,
        ...store.popupQueue.queued,
      ]) {
        if (item.instanceId !== undefined && !wanted.has(item.instanceId))
          store.dismissPopup(item.id);
      }
      popups.forEach((item) => {
        useSmabar.getState().enqueuePopup(item);
      });
    } finally {
      reconciling = false;
    }
    const queue = useSmabar.getState().popupQueue;
    const retained = new Set(
      [...queue.visible, ...queue.queued].map((item) => item.instanceId),
    );
    if (useSmabar.getState().popups.enabled) {
      await Promise.all(
        popups.flatMap((item) =>
          retained.has(item.instanceId)
            ? []
            : [
                invoke("popup_event_report", {
                  instanceId: item.instanceId,
                  state: "dropped",
                }),
              ],
        ),
      );
    }
  };
  const refresh = async () => {
    dirty = true;
    if (refreshing) return;
    refreshing = true;
    try {
      while (dirty) {
        dirty = false;
        await applySnapshot();
      }
    } finally {
      refreshing = false;
    }
  };
  await listen("managed-popups-changed", () => {
    void refresh().catch(reportError);
  });
  // A legacy popup can evict a managed one from the shared bounded queue.
  useSmabar.subscribe((state, previous) => {
    if (
      reconciling ||
      !state.popups.enabled ||
      state.popupQueue === previous.popupQueue
    )
      return;
    const retained = new Set(
      [...state.popupQueue.visible, ...state.popupQueue.queued].map(
        (item) => item.id,
      ),
    );
    for (const item of [
      ...previous.popupQueue.visible,
      ...previous.popupQueue.queued,
    ]) {
      if (!retained.has(item.id))
        void reportPopup(item, "dropped").catch(reportError);
    }
  });
  // Native placement follows measurement; mounting HTML alone is not delivery.
  await listen("notification-placement", () => {
    requestAnimationFrame(() => {
      for (const item of useSmabar.getState().popupQueue.visible) {
        void reportPopup(item, "shown").catch(reportError);
      }
    });
  });
  await refresh();
}
