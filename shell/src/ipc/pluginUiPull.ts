import { invoke } from "@tauri-apps/api/core";

import type { PluginUiEvent } from "./bridge";
import { reportError } from "./log";

/** Overlay pulls carry the flyout generation they were taken for. */
export type PulledPluginUi = PluginUiEvent & { generation?: number };

/**
 * The core only signals pending plugin HTML; the array arrives as a command
 * response (IPC bytes), not as event script source that WebKit keeps. One
 * pull runs at a time; a signal during a pull queues exactly one more.
 */
export function createPluginUiPull(
  apply: (rendered: PulledPluginUi[]) => void,
): () => void {
  let inFlight = false;
  let again = false;
  const run = (): void => {
    inFlight = true;
    void invoke<PulledPluginUi[]>("take_plugin_ui")
      .then(apply, reportError)
      .finally(() => {
        inFlight = false;
        if (again) {
          again = false;
          run();
        }
      });
  };
  return () => {
    if (inFlight) again = true;
    else run();
  };
}
