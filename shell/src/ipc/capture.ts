/**
 * Answers the core's screenshot and UI-state requests.
 *
 * The core cannot see the DOM and the shell cannot snapshot the webview, so
 * every request is a round trip: an event in, a `bar_reply` back with the
 * measured rectangle. Between the reply and `bar-capture-release` the core
 * takes the picture, so the staged layout must survive exactly that window.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { useSmabar } from "../store/bar";
import {
  collectTargets,
  stageCapture,
  type CaptureRect,
} from "./captureTargets";
import { setInputShapePaused } from "./inputShape";
import { reportError } from "./log";
import { closeSettings, openSettings, type SurfaceRole } from "./surface";
import {
  closeFlyoutSurface,
  openFlyoutSurface,
  setOverlayRowOpen,
} from "./overlay";

interface CaptureRequest {
  id: number;
  target: string;
}

type UiAction =
  | "open_flyout"
  | "close_flyout"
  | "open_overlay"
  | "close_overlay"
  | "open_settings"
  | "close_settings";

interface UiCommand {
  id: number;
  action: UiAction;
  tileId: string | null;
  group?: string | null;
}

interface BarReply {
  id: number;
  rect?: CaptureRect;
  expanded?: boolean;
  clipped?: boolean;
  /** `unknown-target` is the one the caller can act on; the core turns it
   *  into an error listing `targets`. */
  error?: string;
  targets: string[];
}

const PAINT_FALLBACK_MS = 100;

function reply(payload: BarReply): void {
  void invoke("bar_reply", { reply: payload }).catch(reportError);
}

/** Two frames: one for the staged layout, one for the paint that follows —
 *  WebKit snapshots what has been rendered, not what has been computed. */
function afterPaint(): Promise<void> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (): void => {
      if (settled) return;
      settled = true;
      window.clearTimeout(fallback);
      resolve();
    };
    const fallback = window.setTimeout(finish, PAINT_FALLBACK_MS);
    requestAnimationFrame(() => {
      requestAnimationFrame(finish);
    });
  });
}

export async function initCapture(surface: SurfaceRole): Promise<void> {
  /** Staging of the request the core is currently photographing. */
  let staged: { id: number; release: () => void } | null = null;
  const active = new Set<number>();
  const cancelled = new Set<number>();
  const eventTarget = { kind: "WebviewWindow", label: surface } as const;

  const clearStage = (id?: number): void => {
    if (staged === null || (id !== undefined && staged.id !== id)) return;
    staged.release();
    staged = null;
    setInputShapePaused(false);
  };

  await listen<CaptureRequest>(
    "bar-capture",
    (event) => {
      const { id, target } = event.payload;
      active.add(id);
      void (async () => {
        // A previous shot that never got released would leak its layout.
        clearStage();
        const targets = collectTargets();
        const subject = targets.get(target);
        if (cancelled.has(id)) return;
        if (subject === undefined) {
          reply({
            id,
            error: "unknown-target",
            targets: [...targets.keys()].filter((name) => name !== target),
          });
          return;
        }
        // The input shape streams DOM rects every frame; a staged subject would
        // push nonsense into the native shape for the duration of the shot.
        const stage = stageCapture(subject);
        setInputShapePaused(true);
        staged = { id, release: stage.release };
        await afterPaint();
        if (cancelled.has(id)) return;
        reply({
          id,
          rect: stage.rect,
          expanded: stage.expanded,
          clipped: stage.clipped,
          targets: [...targets.keys()],
        });
      })()
        .catch((error: unknown) => {
          clearStage(id);
          reportError(error);
          if (!cancelled.has(id)) {
            reply({
              id,
              error: "capture-failed",
              targets: [...collectTargets().keys()],
            });
          }
        })
        .finally(() => {
          active.delete(id);
          cancelled.delete(id);
        });
    },
    { target: eventTarget },
  );

  await listen<number>(
    "bar-capture-release",
    (event) => {
      const id = event.payload;
      if (active.has(id)) cancelled.add(id);
      clearStage(id);
    },
    { target: eventTarget },
  );

  await listen<UiCommand>(
    "bar-ui-command",
    (event) => {
      const { id, action, tileId, group } = event.payload;
      void applyUiAction(action, tileId, group ?? null)
        .then(async (error) => {
          await afterPaint();
          reply({ id, error, targets: [...collectTargets().keys()] });
        })
        .catch((error: unknown) => {
          reportError(error);
          reply({
            id,
            error: "ui-action-failed",
            targets: [...collectTargets().keys()],
          });
        });
    },
    { target: eventTarget },
  );
}

/** Returns an error string when the action could not be applied. */
async function applyUiAction(
  action: UiAction,
  tileId: string | null,
  group: string | null,
): Promise<string | undefined> {
  const store = useSmabar.getState();
  switch (action) {
    case "open_flyout": {
      const flyoutId = tileId ?? "";
      const tile = collectTargets().get(flyoutId);
      if (tile === undefined) return "unknown-target";
      const r = tile.getBoundingClientRect();
      const rect = {
        left: r.left,
        top: r.top,
        width: r.width,
        height: r.height,
      };
      if (!store.openPinnedFlyout(flyoutId, rect)) {
        if (store.reordering) {
          return "a drag is in progress; drop the item or press Escape and retry";
        }
        return "flyout anchor is outside the viewport; reveal the bar and retry";
      }
      await openFlyoutSurface(flyoutId, "pinned", rect);
      return undefined;
    }
    case "close_flyout":
      store.closeFlyout();
      await closeFlyoutSurface();
      return undefined;
    case "open_overlay":
      if (store.layout.variant !== "solo") {
        return "the secondary row requires the solo layout; select solo before opening overlay";
      }
      setOverlayRowOpen(true);
      return undefined;
    case "close_overlay":
      setOverlayRowOpen(false);
      return undefined;
    case "open_settings":
      await openSettings(group ?? undefined);
      return undefined;
    case "close_settings":
      await closeSettings();
      return undefined;
  }
}
