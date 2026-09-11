/** Fallbacks for declarative dialog/popover invokers on older webviews. */
import { behaviour } from "./delegate";

function targetOf(element: HTMLElement, attribute: string): HTMLElement | null {
  const id = element.getAttribute(attribute);
  if (id === null || id === "") return null;
  const root = element.getRootNode();
  if (root instanceof Document || root instanceof ShadowRoot) {
    const target = root.getElementById(id);
    return target instanceof HTMLElement ? target : null;
  }
  return null;
}

function popoverOpen(target: HTMLElement): boolean {
  try {
    return target.matches(":popover-open");
  } catch {
    return false;
  }
}

function showPopover(target: HTMLElement): void {
  if (typeof target.showPopover === "function" && !popoverOpen(target)) {
    target.showPopover();
  }
}

function hidePopover(target: HTMLElement): void {
  if (typeof target.hidePopover === "function" && popoverOpen(target)) {
    target.hidePopover();
  }
}

behaviour("click", "[commandfor]", (invoker, event) => {
  if ("commandForElement" in HTMLButtonElement.prototype) return;
  const target = targetOf(invoker, "commandfor");
  const command = invoker.getAttribute("command")?.toLowerCase();
  if (target === null || command === undefined) return;

  if (target instanceof HTMLDialogElement) {
    if (command === "show-modal" && !target.open) target.showModal();
    else if (command === "close" && target.open) target.close();
    else if (command === "request-close" && target.open) {
      if (typeof target.requestClose === "function") target.requestClose();
      else if (
        target.dispatchEvent(new Event("cancel", { cancelable: true }))
      ) {
        target.close();
      }
    } else return;
  } else if (command === "show-popover") showPopover(target);
  else if (command === "hide-popover") hidePopover(target);
  else if (command === "toggle-popover") {
    if (popoverOpen(target)) hidePopover(target);
    else showPopover(target);
  } else return;
  event.preventDefault();
});

behaviour("click", "[popovertarget]", (invoker, event) => {
  if ("popoverTargetElement" in HTMLButtonElement.prototype) return;
  const target = targetOf(invoker, "popovertarget");
  if (target === null) return;
  const action =
    invoker.getAttribute("popovertargetaction")?.toLowerCase() ?? "toggle";
  if (action === "show") showPopover(target);
  else if (action === "hide") hidePopover(target);
  else if (action === "toggle") {
    if (popoverOpen(target)) hidePopover(target);
    else showPopover(target);
  } else return;
  event.preventDefault();
});
