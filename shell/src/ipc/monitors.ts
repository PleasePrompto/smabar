import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface MonitorOption {
  id: string;
  label: string;
  width: number;
  height: number;
  scaleFactor: number;
  primary: boolean;
}

export interface MonitorState {
  monitors: MonitorOption[];
  effectiveId: string;
  preferredConnected: boolean;
}

export async function getMonitorState(): Promise<MonitorState> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return { monitors: [], effectiveId: "", preferredConnected: true };
  }
  return invoke<MonitorState>("get_monitor_state");
}

export async function onMonitorsChanged(
  handler: () => void,
): Promise<UnlistenFn> {
  if (!("__TAURI_INTERNALS__" in window)) return () => undefined;
  return listen("monitors-changed", handler);
}
