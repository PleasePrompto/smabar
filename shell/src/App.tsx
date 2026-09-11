import { Fragment, useEffect } from "react";

import { BarShell } from "./components/bar/BarShell";
import { DesktopBackground } from "./components/DesktopBackground";
import { DevFixture } from "./components/DevFixture";
import { ContextMenuLayer } from "./components/overlay/ContextMenuLayer";
import { TooltipLayer } from "./components/overlay/Tooltip";
import { PluginPopup } from "./components/PluginPopup";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { Toast } from "./components/Toast";
import { NotificationSurface } from "./components/NotificationSurface";
import { OverlaySurface } from "./components/OverlaySurface";
import { OverlayTooltip } from "./components/OverlayTooltip";
import { reportError } from "./ipc/log";
import { finalizeOverlayClear } from "./ipc/overlay";
import { useSmabar } from "./store/bar";
import { applyTokenOverrides } from "./theme/apply";
import type { AppRole } from "./ipc/surface";

// In the Tauri window the desktop itself is the backdrop; the fake wallpaper
// is only for browser-based development.
const isTauri = "__TAURI_INTERNALS__" in window;

export function App({ role = "browser" }: { role?: AppRole }) {
  // Locale switches remount the tree so every t() call re-evaluates.
  const localeVersion = useSmabar((s) => s.localeVersion);
  useEffect(() => {
    // Tokens affect the DOM directly. A React subscription here used to
    // re-render the entire app for every slider input before applying them.
    // The vanilla store subscription keeps :root in sync in both runtimes
    // without pulling unrelated shell and settings trees through React.
    let current = useSmabar.getState().appearance.tokens;
    applyTokenOverrides(current);
    return useSmabar.subscribe((state) => {
      const next = state.appearance.tokens;
      if (next === current) return;
      current = next;
      applyTokenOverrides(next);
    });
  }, []);
  let content;
  if (role === "bar") {
    content = (
      <>
        <BarShell />
        <ContextMenuLayer surface="bar" />
        <TooltipLayer />
      </>
    );
  } else if (role === "settings") {
    content = <SettingsPanel />;
  } else if (role === "notifications") {
    content = <NotificationSurface />;
  } else if (role === "overlay") {
    content = (
      <>
        <OverlaySurface />
        <ContextMenuLayer surface="overlay" />
        <OverlayTooltip />
        <TooltipLayer inline />
        <OverlayClearBarrier />
      </>
    );
  } else {
    content = <BrowserPreview />;
  }
  return (
    <Fragment key={localeVersion}>
      {!isTauri && import.meta.env.DEV && <DesktopBackground />}
      {content}
      {role === "browser" && <DevFixture />}
    </Fragment>
  );
}

function OverlayClearBarrier() {
  useEffect(() => {
    const root = document.getElementById("root");
    if (root === null) return;
    const observer = new MutationObserver(() => {
      if (root.childElementCount === 0) {
        void finalizeOverlayClear().catch(reportError);
      }
    });
    observer.observe(root, { childList: true });
    return () => {
      observer.disconnect();
    };
  }, []);
  return null;
}

function BrowserPreview() {
  const settingsOpen = useSmabar((s) => s.settingsOpen);
  useEffect(() => {
    const open = (event: Event) => {
      const detail = (event as CustomEvent<{ group?: string }>).detail;
      if (detail.group !== undefined) {
        useSmabar.getState().setSettingsGroup(detail.group);
      }
      useSmabar.getState().setSettingsOpen(true);
    };
    const close = () => {
      useSmabar.getState().setSettingsOpen(false);
    };
    window.addEventListener("smabar-preview-settings", open);
    window.addEventListener("smabar-preview-settings-close", close);
    return () => {
      window.removeEventListener("smabar-preview-settings", open);
      window.removeEventListener("smabar-preview-settings-close", close);
    };
  }, []);
  return (
    <>
      <BarShell />
      <PluginPopup />
      {settingsOpen && <SettingsPanel preview />}
      <ContextMenuLayer surface="bar" />
      <TooltipLayer />
      <Toast />
    </>
  );
}
