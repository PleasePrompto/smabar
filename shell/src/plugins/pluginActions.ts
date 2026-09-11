import { call } from "../ipc/call";
import { reportError } from "../ipc/log";

import { KIT_INTERACTIVE } from "./behaviour";

/** Sends one declared plugin action; failures go through centralized logging. */
export function sendPluginAction(
  pluginId: string,
  tileId: string,
  action: string,
  value?: string | Record<string, string>,
  popupInstanceId?: number,
): void {
  void call("plugin_action", {
    pluginId,
    tileId,
    action,
    value,
    ...(popupInstanceId === undefined ? {} : { popupInstanceId }),
  }).catch(reportError);
}

/** True when the browser or UI kit itself acts on this click. */
export function handlesItsOwnClick(element: HTMLElement): boolean {
  if (element.hasAttribute("popovertarget")) return true;
  if (element.hasAttribute("commandfor")) return true;
  if (element.matches(KIT_INTERACTIVE)) return true;
  return ["summary", "select", "option", "textarea", "label", "input"].includes(
    element.localName,
  );
}

/** First plugin action before a native/kit control owns the click. */
export function actionElementInPath(
  path: EventTarget[],
  boundary: EventTarget,
): HTMLElement | null {
  for (const hop of path) {
    if (hop === boundary) return null;
    if (!(hop instanceof HTMLElement)) continue;
    if (hop.dataset.action !== undefined) return hop;
    if (handlesItsOwnClick(hop)) return null;
  }
  return null;
}

/** Form action declared by its submitter, if any. */
export function submitIntent(
  target: EventTarget | null,
  submitter: HTMLElement | null = null,
): { action: string; value?: string } | null {
  if (!(target instanceof HTMLFormElement)) return null;
  const button =
    submitter ??
    target.querySelector<HTMLElement>(
      'button[data-action]:not([type="button"]):not([type="reset"]), input[type="submit"][data-action]',
    );
  const action = button?.dataset.action;
  if (action === undefined) return null;
  const value = button?.dataset.value;
  return value === undefined ? { action } : { action, value };
}
