import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { useSmabar, type FlyoutMode, type FlyoutRect } from "../store/bar";
import type { ContextMenuItem } from "../components/overlay/model";
import { uiLog } from "./log";

export interface FlyoutRequest {
  generation: number;
  tileId: string;
  mode: FlyoutMode;
  preserveContent?: boolean;
}

export interface FlyoutPlacement {
  generation: number;
  direction: "up" | "down";
  pointerX: number;
  x: number;
  y: number;
}

export interface FlyoutMeasure {
  generation: number;
  width: number;
  height: number;
  inset: number;
  gap: number;
  pointerReserve: number;
}

export interface MenuRequest {
  generation: number;
  items: ContextMenuItem[];
}

export interface MenuPlacement {
  generation: number;
  anchorX: number;
  anchorY: number;
}

export interface MenuMeasure {
  generation: number;
  width: number;
  height: number;
  inset: number;
}

export interface TooltipRequest {
  generation: number;
  text: string;
}

export interface TooltipPlacement {
  generation: number;
  side: "top" | "bottom";
  x: number;
  y: number;
}

export interface TooltipMeasure {
  generation: number;
  width: number;
  height: number;
  inset: number;
  gap: number;
}

let commandQueue = Promise.resolve();
const pointerSubscribers = new Set<(inside: boolean) => void>();

/**
 * A surface root without layout — content not rendered yet, or already
 * cleared — has nothing to place. The core rejects such a measure, so it must
 * never leave the shell; the ResizeObserver reports again once the root has a
 * size.
 */
export function hasLayout(size: { width: number; height: number }): boolean {
  return size.width > 0 && size.height > 0;
}

export function skipUnmeasured(
  surface: string,
  generation: number,
): Promise<void> {
  uiLog("debug", `skipped a ${surface} measure without layout`, {
    fields: { surface, generation },
  });
  return Promise.resolve();
}

function orderedInvoke(command: string, args?: Record<string, unknown>) {
  if (!("__TAURI_INTERNALS__" in window)) return Promise.resolve();
  const pending = commandQueue.then(() => invoke(command, args));
  commandQueue = pending.then(
    () => undefined,
    () => undefined,
  );
  return pending;
}

export function openFlyoutSurface(
  tileId: string,
  mode: FlyoutMode,
  trigger: FlyoutRect,
): Promise<unknown> {
  return orderedInvoke("open_flyout", {
    tileId,
    mode,
    trigger: {
      x: Math.round(trigger.left),
      y: Math.round(trigger.top),
      w: Math.round(trigger.width),
      h: Math.round(trigger.height),
    },
  });
}

export function closeFlyoutSurface(generation?: number): Promise<unknown> {
  return orderedInvoke("close_flyout", { generation });
}

export function finalizeOverlayClear(): Promise<unknown> {
  return orderedInvoke("finalize_overlay_clear");
}

export function pinFlyoutSurface(
  generation: number,
  replaceContent: boolean,
): Promise<unknown> {
  return orderedInvoke("pin_flyout", { generation, replaceContent });
}

export function reportFlyoutMeasure(measure: FlyoutMeasure): Promise<unknown> {
  if (!hasLayout(measure)) return skipUnmeasured("flyout", measure.generation);
  return orderedInvoke("measure_flyout", { measure });
}

export function reportOverlayPointer(inside: boolean): Promise<unknown> {
  return orderedInvoke("set_overlay_pointer", { inside });
}

export function openContextMenuSurface(
  items: ContextMenuItem[],
  anchor: { x: number; y: number },
  keepFlyout: boolean,
): Promise<unknown> {
  return orderedInvoke("open_context_menu", {
    items,
    anchor: { x: Math.round(anchor.x), y: Math.round(anchor.y) },
    keepFlyout,
  });
}

export function reportContextMenuMeasure(
  measure: MenuMeasure,
): Promise<unknown> {
  if (!hasLayout(measure)) {
    return skipUnmeasured("context menu", measure.generation);
  }
  return orderedInvoke("measure_context_menu", { measure });
}

export function closeContextMenuSurface(generation?: number): Promise<unknown> {
  return orderedInvoke("close_context_menu", { generation });
}

export function openTooltipSurface(
  text: string,
  trigger: FlyoutRect,
): Promise<unknown> {
  return orderedInvoke("open_tooltip", {
    text,
    trigger: {
      x: Math.round(trigger.left),
      y: Math.round(trigger.top),
      w: Math.round(trigger.width),
      h: Math.round(trigger.height),
    },
  });
}

export function reportTooltipMeasure(
  measure: TooltipMeasure,
): Promise<unknown> {
  if (!hasLayout(measure)) {
    return skipUnmeasured("tooltip", measure.generation);
  }
  return orderedInvoke("measure_tooltip", { measure });
}

export function closeTooltipSurface(generation?: number): Promise<unknown> {
  return orderedInvoke("close_tooltip", { generation });
}

/**
 * The solo layout's second row renders inside the bar window, so opening it
 * is a store change the bar shell renders — no surface of its own.
 */
export function setOverlayRowOpen(open: boolean): void {
  useSmabar.getState().setOverlayOpen(open);
}

export function subscribeOverlayPointer(
  subscriber: (inside: boolean) => void,
): () => void {
  pointerSubscribers.add(subscriber);
  return () => {
    pointerSubscribers.delete(subscriber);
  };
}

export function publishOverlayPointer(inside: boolean): void {
  pointerSubscribers.forEach((subscriber) => {
    subscriber(inside);
  });
}

export async function initBarOverlayEvents(): Promise<void> {
  await listen<FlyoutRequest>("flyout-pinned", (event) => {
    const store = useSmabar.getState();
    if (store.openFlyout === event.payload.tileId) store.pinFlyout();
  });
  await listen<FlyoutRequest>("flyout-closed", (event) => {
    const store = useSmabar.getState();
    if (store.openFlyout === event.payload.tileId) store.closeFlyout();
  });
  await listen<boolean>("overlay-pointer", (event) => {
    publishOverlayPointer(event.payload);
  });
}
