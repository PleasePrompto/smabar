import { emitTo, listen } from "@tauri-apps/api/event";

import { useSmabar } from "../store/bar";
import type { SmabarState } from "../store/barState";
import { checkUpdate, installUpdate } from "./update";
import {
  openSettings,
  stageNotificationUpdate,
  type SurfaceRole,
} from "./surface";
import { reportError } from "./log";

type UpdateAction =
  | { action: "check" | "details" }
  | { action: "install" | "dismiss"; version: string };

type Snapshot = Pick<
  SmabarState,
  "updateChannel" | "updateStatus" | "updateOffer" | "dismissedUpdateVersion"
> & { revision: number };

/** The bar owns this session; other windows only request actions and mirror it. */
async function handleAction(payload: unknown): Promise<void> {
  if (typeof payload !== "object" || payload === null || !("action" in payload))
    throw new Error("Invalid application update request.");
  const state = useSmabar.getState();
  if (state.updateChannel !== "app" || state.legalRequired) return;
  switch (payload.action) {
    case "check":
      await checkUpdate();
      return;
    case "details":
      await openSettings("system/updates");
      return;
    case "dismiss":
    case "install": {
      if (!("version" in payload) || typeof payload.version !== "string")
        throw new Error("Application update request needs a version.");
      if (payload.version !== state.updateOffer?.version) return;
      if (payload.action === "dismiss") {
        useSmabar.setState({ dismissedUpdateVersion: payload.version });
      } else if (state.updateOffer.installer !== null) {
        void openSettings("system/updates").catch(reportError);
        await installUpdate(payload.version);
      }
      return;
    }
    default:
      throw new Error("Unknown application update action.");
  }
}

export async function requestUpdate(request: UpdateAction): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) return handleAction(request);
  await emitTo("bar", "app-update-action", request);
}

export function hasAppUpdate(state: SmabarState): boolean {
  return (
    state.updateChannel === "app" &&
    state.updateOffer !== null &&
    state.updateStatus.state !== "handedOff"
  );
}

export function showUpdateNotification(state: SmabarState): boolean {
  return (
    hasAppUpdate(state) &&
    !state.legalRequired &&
    state.updateOffer?.version !== state.dismissedUpdateVersion &&
    state.updateStatus.state !== "downloading" &&
    state.updateStatus.state !== "installing"
  );
}

/** Subscribe before requesting a snapshot; revisions reject late responses. */
export async function initUpdateSync(role: SurfaceRole): Promise<() => void> {
  if (role === "bar") {
    let revision = 0;
    const publish = async (notify = true) => {
      const {
        updateChannel,
        updateStatus,
        updateOffer,
        dismissedUpdateVersion,
      } = useSmabar.getState();
      const snapshot: Snapshot = {
        revision: ++revision,
        updateChannel,
        updateStatus,
        updateOffer,
        dismissedUpdateVersion,
      };
      await Promise.all(
        (notify ? ["settings", "notifications"] : ["settings"]).map((target) =>
          emitTo(target, `app-update-${target}`, snapshot),
        ),
      );
    };
    const stopSnapshots = await listen("app-update-snapshot", () => {
      void publish().catch(reportError);
    });
    const stopActions = await listen<unknown>("app-update-action", (event) => {
      void handleAction(event.payload).catch(reportError);
    });
    const stopState = useSmabar.subscribe((state, previous) => {
      const offerChanged =
        state.updateChannel !== previous.updateChannel ||
        state.updateOffer !== previous.updateOffer ||
        state.dismissedUpdateVersion !== previous.dismissedUpdateVersion;
      if (!offerChanged && state.updateStatus === previous.updateStatus) return;
      // The notification is hidden during a download; byte progress only
      // belongs to Settings, without native remeasurement for each chunk.
      const progressOnly =
        !offerChanged &&
        state.updateStatus.state === "downloading" &&
        previous.updateStatus.state === "downloading";
      void publish(!progressOnly).catch(reportError);
    });
    void publish().catch(reportError);
    return () => {
      stopSnapshots();
      stopActions();
      stopState();
    };
  } else if (role === "settings" || role === "notifications") {
    let revision = -1;
    const apply = async (snapshot: Snapshot) => {
      if (snapshot.revision <= revision) return;
      revision = snapshot.revision;
      if (
        role === "notifications" &&
        showUpdateNotification(useSmabar.getState())
      )
        await stageNotificationUpdate();
      if (snapshot.revision !== revision) return;
      const {
        updateChannel,
        updateStatus,
        updateOffer,
        dismissedUpdateVersion,
      } = snapshot;
      useSmabar.setState({
        updateChannel,
        updateStatus,
        updateOffer,
        dismissedUpdateVersion,
      });
    };
    const stop = await listen<Snapshot>(`app-update-${role}`, (event) => {
      void apply(event.payload).catch(reportError);
    });
    void emitTo("bar", "app-update-snapshot").catch(reportError);
    return stop;
  }
  return () => undefined;
}
