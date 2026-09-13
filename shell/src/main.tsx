import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import defaultTheme from "../../themes/default.json";
import { App } from "./App";
import { initBridge } from "./ipc/bridge";
import { initCapture } from "./ipc/capture";
import { initDragDrop } from "./ipc/dragDrop";
import { initInputShape } from "./ipc/inputShape";
import {
  currentAppRole,
  initSurface,
  markSurfaceReady,
  type SurfaceRole,
} from "./ipc/surface";
import { onStoreChanged, refreshCommunityBadge } from "./ipc/store";
import { initUpdates } from "./ipc/update";
import { initBarOverlayEvents } from "./ipc/overlay";
import { applyTheme } from "./theme/apply";
import { readThemeDocument } from "./theme/document";
import { logWebviewCapabilities } from "./ipc/capabilities";
import { installGlobalErrorReporting, reportError } from "./ipc/log";
import { installKitBehaviour } from "./plugins/behaviour";
import "./styles/globals.css";

// Boot-apply the bundled default theme before first paint so every --sb-*
// token is always set (browser-dev included); the core's resolved theme from
// get_ui_state overwrites these moments later in the Tauri window.
applyTheme(readThemeDocument(defaultTheme).tokens);

// The plugin kit's behaviour: one set of delegated listeners for every plugin
// shadow root in the window. Plugins ship markup, the shell ships the script.
installKitBehaviour();

const role = currentAppRole();
const root = document.getElementById("root");
if (root === null) {
  throw new Error("missing #root element in index.html");
}
const mounted = new Promise<void>((resolve) => {
  createRoot(root).render(
    <StrictMode>
      <App role={role} onMounted={resolve} />
    </StrictMode>,
  );
});

if ("__TAURI_INTERNALS__" in window) {
  installGlobalErrorReporting();
  logWebviewCapabilities();
  void initTauriSurface(role as SurfaceRole).catch(reportError);
}

async function initTauriSurface(surface: SurfaceRole): Promise<void> {
  await initSurface(surface);
  if (surface === "bar") initInputShape();
  await initBridge(surface);
  await initCapture(surface);
  if (surface === "bar") {
    await initBarOverlayEvents();
    await initDragDrop(surface);
    await initUpdates(true);
    // The core refreshes the catalog on its own timer and announces every
    // change; the bar only counts. One initial count covers the events that
    // fired before this listener existed.
    await onStoreChanged(() => {
      void refreshCommunityBadge();
    });
    await refreshCommunityBadge();
  } else if (surface === "settings") {
    await initDragDrop(surface);
    await initUpdates(false);
  }
  await mounted;
  await markSurfaceReady();
}
