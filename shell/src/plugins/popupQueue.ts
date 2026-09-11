/**
 * Pure logic of the proactive popup stack: several toasts are visible at
 * once (capped, bounded overflow waits in a FIFO queue), each with an independent
 * lifetime — a numeric ttl auto-dismisses, a missing/null ttl is sticky
 * until the user clicks the X.
 */

export const POPUP_TTL_MIN_MS = 1_000;
export const POPUP_TTL_MAX_MS = 120_000;
export const POPUP_VISIBLE_MAX = 5;
export const POPUP_QUEUED_MAX = 45;
export const POPUP_TOTAL_MAX = POPUP_VISIBLE_MAX + POPUP_QUEUED_MAX;

/** Where the popup stack docks (mirrors `config::PopupPosition`). */
export type PopupPosition =
  | "top-left"
  | "top-center"
  | "top-right"
  | "bottom-left"
  | "bottom-center"
  | "bottom-right";

/** Screen edge the stack grows away from. */
export function popupVertical(position: PopupPosition): "top" | "bottom" {
  return position.startsWith("top") ? "top" : "bottom";
}

/** Horizontal anchoring of the stack. */
export function popupHorizontal(
  position: PopupPosition,
): "left" | "center" | "right" {
  if (position.endsWith("left")) return "left";
  if (position.endsWith("right")) return "right";
  return "center";
}

export interface PopupRequest {
  /** Stable host instance; absent on legacy fire-and-forget popups. */
  instanceId?: number;
  pluginId: string;
  tileId: string;
  html: string;
  /** Auto-dismiss timeout in ms; missing/null = sticky. */
  ttlMs?: number | null;
}

export interface PopupItem {
  instanceId?: number;
  id: number;
  pluginId: string;
  tileId: string;
  html: string;
  /** Clamped auto-dismiss timeout; null = sticky until dismissed. */
  ttlMs: number | null;
  /** When the toast became visible — the component derives its initial
   *  lifetime from this before accounting for interaction pauses. */
  shownAtMs: number;
}

export interface PopupStackState {
  /** Visible toasts in arrival order (index 0 = oldest). */
  visible: PopupItem[];
  /** Bounded overflow beyond {@link POPUP_VISIBLE_MAX}, in arrival order. */
  queued: PopupItem[];
}

export const EMPTY_POPUP_STACK: PopupStackState = { visible: [], queued: [] };

/** Sticky for missing/broken values, otherwise clamped to 1–120 s. */
export function clampPopupTtl(ttlMs: number | null | undefined): number | null {
  if (ttlMs === undefined || ttlMs === null || !Number.isFinite(ttlMs)) {
    return null;
  }
  return Math.min(
    POPUP_TTL_MAX_MS,
    Math.max(POPUP_TTL_MIN_MS, Math.round(ttlMs)),
  );
}

/** Adds a popup; a full queue drops its oldest waiting item for the newest. */
export function enqueuePopup(
  state: PopupStackState,
  request: PopupRequest,
  nowMs: number,
  id: number,
): PopupStackState {
  if (request.instanceId !== undefined) {
    const existing = [...state.visible, ...state.queued].find(
      (item) => item.instanceId === request.instanceId,
    );
    if (existing !== undefined) {
      const update = (item: PopupItem): PopupItem =>
        item === existing
          ? { ...item, html: request.html, ttlMs: clampPopupTtl(request.ttlMs) }
          : item;
      return {
        visible: state.visible.map(update),
        queued: state.queued.map(update),
      };
    }
  }
  const item: PopupItem = {
    ...(request.instanceId === undefined
      ? {}
      : { instanceId: request.instanceId }),
    id,
    pluginId: request.pluginId,
    tileId: request.tileId,
    html: request.html,
    ttlMs: clampPopupTtl(request.ttlMs),
    shownAtMs: nowMs,
  };
  if (state.visible.length < POPUP_VISIBLE_MAX) {
    return { visible: [...state.visible, item], queued: state.queued };
  }
  const queued =
    state.queued.length < POPUP_QUEUED_MAX
      ? [...state.queued, item]
      : [...state.queued.slice(1), item];
  return { visible: state.visible, queued };
}

/**
 * Removes one popup by id (visible or queued) and promotes queued popups
 * into the freed visible slots. Promoted popups start their timer at
 * `nowMs`; every other visible popup keeps its original `shownAtMs`.
 */
export function dismissPopup(
  state: PopupStackState,
  id: number,
  nowMs: number,
): PopupStackState {
  const visible = state.visible.filter((item) => item.id !== id);
  const queued = state.queued.filter((item) => item.id !== id);
  if (
    visible.length === state.visible.length &&
    queued.length === state.queued.length
  ) {
    return state;
  }
  const slots = Math.min(POPUP_VISIBLE_MAX - visible.length, queued.length);
  const promoted = queued
    .slice(0, slots)
    .map((item) => ({ ...item, shownAtMs: nowMs }));
  return { visible: [...visible, ...promoted], queued: queued.slice(slots) };
}

/** Ids of visible popups whose timer has elapsed at `nowMs` (sticky: never). */
export function duePopups(state: PopupStackState, nowMs: number): number[] {
  return state.visible
    .filter(
      (item) => item.ttlMs !== null && nowMs - item.shownAtMs >= item.ttlMs,
    )
    .map((item) => item.id);
}
