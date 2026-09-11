import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

import {
  pushPointerSample,
  subscribePointerSamples,
} from "../components/bar/useAutohide";
import { reportError } from "./log";
import { useSmabar } from "../store/bar";
import { showNotice, type SurfaceRole } from "./surface";

/**
 * Progressive-enhancement pinning: dropping files or folders onto the shortcut
 * zone pins them (failed drops get a transient hint). The
 * settings search stays the primary path — XDND onto an input-shaped dock
 * window is unproven; the core traces every drag-drop window event so a
 * silent XDND failure is diagnosable from the logs.
 *
 * The drag position doubles as a pointer source for the autohide bar: while
 * X11 owns the pointer for the drag the webview receives no mouse events at
 * all, so this is the ONLY way a hidden bar learns that something is being
 * dragged towards its screen edge.
 */

/** Client-space point; drag positions arrive in physical device pixels. */
interface Point {
  x: number;
  y: number;
}

function toClientPoint(position: Point): Point {
  const dpr = window.devicePixelRatio || 1;
  return { x: position.x / dpr, y: position.y / dpr };
}

/** Whether a client-space point falls inside the shortcut zone. */
function overShortcutZone(point: Point): boolean {
  const zone = document.querySelector("[data-shortcut-zone]");
  if (zone === null) return false;
  const rect = zone.getBoundingClientRect();
  return (
    point.x >= rect.left &&
    point.x <= rect.right &&
    point.y >= rect.top &&
    point.y <= rect.bottom
  );
}

async function handleDrop(paths: string[]): Promise<void> {
  let failed = false;
  for (const path of paths) {
    // Every drop goes to the core: the path string alone cannot tell a
    // folder from an odd file (only the core can stat it), and its
    // validation owns platform-specific handling. Await each mutation so a
    // multi-file drop keeps the order supplied by Tauri; one failure does not
    // prevent the remaining paths from being pinned.
    try {
      await invoke("pin_shortcut", { path });
    } catch (error: unknown) {
      failed = true;
      reportError(error);
    }
  }
  if (failed) {
    void showNotice("shortcuts.dropFailed").catch(reportError);
  }
}

/** Ends the drag: no position is known any more, the bar may retract. */
function endDrag(): void {
  const store = useSmabar.getState();
  store.setFileDrag(false);
  store.setDropActive(false);
  pushPointerSample(Number.NaN, Number.NaN);
}

/** Attaches the window drag-drop listener (Tauri window only, app lifetime). */
export async function initDragDrop(role: SurfaceRole): Promise<void> {
  // Native DnD can miss Drop/Leave or deliver Enter after release. Reuse
  // native pointer exit and ordinary input to end the visual drag state.
  subscribePointerSamples((x, y) => {
    if (
      (!Number.isFinite(x) || !Number.isFinite(y)) &&
      useSmabar.getState().fileDrag
    ) {
      endDrag();
    }
  });
  const endReleasedDrag = (event: PointerEvent): void => {
    if (event.buttons === 0 && useSmabar.getState().fileDrag) endDrag();
  };
  document.addEventListener("pointermove", endReleasedDrag, {
    capture: true,
    passive: true,
  });
  document.addEventListener("pointerup", endReleasedDrag, {
    capture: true,
    passive: true,
  });
  document.addEventListener(
    "keydown",
    (event) => {
      if (event.key === "Escape" && useSmabar.getState().fileDrag) endDrag();
    },
    true,
  );

  await getCurrentWebviewWindow().onDragDropEvent((event) => {
    const store = useSmabar.getState();
    const payload = event.payload;
    if (payload.type === "enter" || payload.type === "over") {
      const point = toClientPoint(payload.position);
      // Feed the reveal rule first: a hidden bar only receives these events
      // while the drag is over its edge hot strip (the sole part of the
      // input shape that exists then), and the reveal is what puts the drop
      // target under the pointer in the first place.
      pushPointerSample(point.x, point.y);
      store.setFileDrag(true);
      store.setDropActive(overShortcutZone(point));
    } else if (payload.type === "drop") {
      const point = toClientPoint(payload.position);
      endDrag();
      // A .json dropped while the settings are open is a theme file: it goes
      // through the theme manager's normal import flow (collision confirms
      // included) instead of the pinning path.
      const themeFile =
        role === "settings"
          ? payload.paths.find((path) => path.toLowerCase().endsWith(".json"))
          : undefined;
      if (themeFile !== undefined) {
        store.setThemeImportPath(themeFile);
      } else if (overShortcutZone(point)) {
        void handleDrop(payload.paths);
      }
    } else {
      endDrag();
    }
  });
}
