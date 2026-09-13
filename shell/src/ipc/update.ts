import { listen } from "@tauri-apps/api/event";

import { useSmabar, type UpdateStatus } from "../store/bar";
import type { UpdateInfo } from "../store/types";
import { call } from "./call";
import { describeError, uiLog } from "./log";

/** `install_update`'s answer on Linux; Windows exits and macOS restarts instead. */
interface HandOff {
  path: string;
  opened: boolean;
}

interface UpdateProgress {
  received: number;
  total: number | null;
  finished: boolean;
}

/** Well after startup, so the check never competes with the bar coming up. */
export const FIRST_CHECK_MS = 60_000;
/** A release is never urgent. */
export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

/** Failure disables only application updates, not the rest of shell startup. */
export async function initUpdates(background: boolean): Promise<void> {
  useSmabar.setState({ updateChannel: null });
  try {
    const system = await call<{ updateChannel: unknown }>(
      "get_system_settings",
    );
    const channel = system.updateChannel;
    if (channel !== "app" && channel !== "store") {
      throw new Error("Unknown application update channel; restart smabar.");
    }
    useSmabar.setState({ updateChannel: channel });
    if (channel !== "app") return;
    await initUpdateEvents();
    if (background) scheduleUpdateChecks();
  } catch (error) {
    uiLog("error", "Application updates could not start; restart smabar.", {
      fields: { cause: describeError(error) },
    });
  }
}

/** A running install owns the row; checks and a second install wait. */
function busy(status: UpdateStatus): boolean {
  return (
    status.state === "checking" ||
    status.state === "downloading" ||
    status.state === "installing"
  );
}

/**
 * Runs one check and mirrors the outcome into the store. Never throws: a
 * failed check is a state the settings row shows, and the core already
 * logged the cause.
 */
export async function checkUpdate(): Promise<void> {
  const store = useSmabar.getState();
  if (store.updateChannel !== "app" || busy(store.updateStatus)) return;
  store.setUpdateStatus({ state: "checking" });
  let status: UpdateStatus;
  let updateOffer = store.updateOffer;
  try {
    const info = await call<UpdateInfo | null>("check_update");
    updateOffer = info;
    status =
      info === null ? { state: "current" } : { state: "available", ...info };
  } catch (error) {
    status = { state: "failed", phase: "check", message: describeError(error) };
  }
  useSmabar.setState({ updateStatus: status, updateOffer });
}

/**
 * Downloads and applies the release the user saw. Progress arrives through
 * `update-progress`; on Windows and macOS the process ends while this call
 * is still pending, so `installing` is the last state the shell shows there.
 */
export async function installUpdate(version: string): Promise<void> {
  const store = useSmabar.getState();
  if (store.updateChannel !== "app" || busy(store.updateStatus)) return;
  store.setUpdateStatus({
    state: "downloading",
    version,
    received: 0,
    total: null,
  });
  try {
    const handOff = await call<HandOff>("install_update", {
      expectedVersion: version,
    });
    useSmabar
      .getState()
      .setUpdateStatus({ state: "handedOff", version, ...handOff });
  } catch (error) {
    useSmabar.getState().setUpdateStatus({
      state: "failed",
      phase: "install",
      message: describeError(error),
    });
  }
}

/**
 * Mirrors download progress. Only a running download is updated, so an
 * event that straggles in after the command already failed cannot revive it.
 */
export async function initUpdateEvents(): Promise<void> {
  if (useSmabar.getState().updateChannel !== "app") return;
  await listen<UpdateProgress>("update-progress", (event) => {
    const store = useSmabar.getState();
    const current = store.updateStatus;
    if (current.state !== "downloading") return;
    const { received, total, finished } = event.payload;
    store.setUpdateStatus(
      finished
        ? { state: "installing", version: current.version }
        : { state: "downloading", version: current.version, received, total },
    );
  });
}

/** Background checks: one delayed first run, then periodic. */
export function scheduleUpdateChecks(): void {
  if (useSmabar.getState().updateChannel !== "app") return;
  window.setTimeout(() => {
    void checkUpdate();
    window.setInterval(() => {
      void checkUpdate();
    }, CHECK_INTERVAL_MS);
  }, FIRST_CHECK_MS);
}
