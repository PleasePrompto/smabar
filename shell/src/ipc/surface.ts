import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { useSmabar } from "../store/bar";
import { reportError } from "./log";
import { requestBarGeometry } from "./inputShape";
import { hasLayout, skipUnmeasured } from "./overlay";

export type SurfaceRole = "bar" | "overlay" | "settings" | "notifications";
export type AppRole = SurfaceRole | "browser";

interface SurfaceContext {
  role: SurfaceRole;
  workAreaWidth: number;
  workAreaHeight: number;
  settingsOpen: boolean;
}

interface SettingsRequest {
  group: string;
}

interface SurfaceSize {
  width: number;
  height: number;
}

export interface NotificationMeasure {
  popup: SurfaceSize | null;
  notice: SurfaceSize | null;
  edgeInset: number;
  gap: number;
}

const SURFACE_ROLES = new Set<string>([
  "bar",
  "overlay",
  "settings",
  "notifications",
]);

export function currentAppRole(): AppRole {
  if (!("__TAURI_INTERNALS__" in window)) return "browser";
  const label = getCurrentWindow().label;
  if (!SURFACE_ROLES.has(label)) {
    throw new Error(`unknown smabar surface label: ${label}`);
  }
  return label as SurfaceRole;
}

export async function initSurface(role: SurfaceRole): Promise<void> {
  let settingsChanged = false;
  if (role === "bar") {
    await listen<boolean>("settings-open-changed", (event) => {
      settingsChanged = true;
      useSmabar.getState().setSettingsOpen(event.payload);
    });
  }
  if (role === "settings") {
    await listen<SettingsRequest>("surface-settings", (event) => {
      useSmabar.getState().setSettingsGroup(event.payload.group);
    });
  }
  const applyContext = (context: SurfaceContext) => {
    // An event received while the initial snapshot was in flight is newer.
    if (role === "bar" && !settingsChanged) {
      useSmabar.getState().setSettingsOpen(context.settingsOpen);
    }
    document.documentElement.dataset.sbSurface = context.role;
    document.documentElement.style.setProperty(
      "--sb-work-area-width",
      `${String(context.workAreaWidth)}px`,
    );
    document.documentElement.style.setProperty(
      "--sb-work-area-height",
      `${String(context.workAreaHeight)}px`,
    );
  };
  await listen("surface-context-changed", () => {
    invoke<SurfaceContext>("get_surface_context")
      .then((context) => {
        applyContext(context);
        if (role === "bar") requestBarGeometry();
      })
      .catch(reportError);
  });
  const context = await invoke<SurfaceContext>("get_surface_context");
  applyContext(context);
}

export async function markSurfaceReady(): Promise<void> {
  await invoke("surface_ready", { devicePixelRatio: window.devicePixelRatio });
}

export async function setBarRevealed(
  revealed: boolean,
  durationMs: number,
  visibleHeight: number,
): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) return;
  await invoke("set_bar_revealed", {
    revealed,
    durationMs: Math.max(0, Math.round(durationMs)),
    visibleHeight: Math.max(1, Math.ceil(visibleHeight)),
  });
}

export async function openSettings(group = "bar"): Promise<void> {
  if ("__TAURI_INTERNALS__" in window) {
    await invoke("open_settings", { group });
    return;
  }
  window.dispatchEvent(
    new CustomEvent("smabar-preview-settings", { detail: { group } }),
  );
}

export async function toggleSettings(): Promise<void> {
  if ("__TAURI_INTERNALS__" in window) {
    await invoke("toggle_settings");
    return;
  }
  window.dispatchEvent(
    new CustomEvent("smabar-preview-settings", {
      detail: { group: "bar" },
    }),
  );
}

export async function closeSettings(): Promise<void> {
  if ("__TAURI_INTERNALS__" in window) {
    await invoke("close_settings");
    return;
  }
  window.dispatchEvent(new CustomEvent("smabar-preview-settings-close"));
}

export async function closeCurrentSurface(): Promise<void> {
  if ("__TAURI_INTERNALS__" in window) {
    await invoke("close_surface");
    return;
  }
  window.dispatchEvent(new CustomEvent("smabar-preview-settings-close"));
}

export async function showNotice(key: string, ttlMs = 3_000): Promise<void> {
  if ("__TAURI_INTERNALS__" in window) {
    await invoke("show_notice", { key, ttlMs });
    return;
  }
  useSmabar.getState().flashNotice(key, ttlMs);
}

export async function reportNotificationMeasure(
  measure: NotificationMeasure,
): Promise<void> {
  // A popup or notice that is mounted but not laid out yet reports 0×0; the
  // core rejects that, and the ResizeObserver reports again once it has a size.
  const unmeasured = [measure.popup, measure.notice].some(
    (size) => size !== null && !hasLayout(size),
  );
  if (unmeasured) {
    await skipUnmeasured("notification", 0);
    return;
  }
  await invoke("set_notification_measure", { measure });
}

/** Conceals the native notification window before its DOM changes. */
export async function stageNotificationUpdate(): Promise<void> {
  if ("__TAURI_INTERNALS__" in window) {
    await invoke("stage_notification_update");
  }
}
