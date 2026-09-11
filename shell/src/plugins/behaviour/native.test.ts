// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { installKitBehaviour } from "./index";

let host: HTMLDivElement;
let root: ShadowRoot;
let teardown: () => void;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = host.attachShadow({ mode: "open" });
  teardown = installKitBehaviour();
});

afterEach(() => {
  teardown();
  host.remove();
});

function click(element: Element): MouseEvent {
  const event = new MouseEvent("click", {
    bubbles: true,
    cancelable: true,
    composed: true,
  });
  element.dispatchEvent(event);
  return event;
}

test("commandfor falls back to showModal and close when invokers are absent", () => {
  root.innerHTML =
    '<button id="open" commandfor="dialog" command="show-modal"></button>' +
    '<button id="close" commandfor="dialog" command="close"></button>' +
    '<dialog id="dialog"></dialog>';
  const dialog = root.querySelector("dialog");
  const open = root.querySelector("#open");
  const close = root.querySelector("#close");
  if (dialog === null || open === null || close === null) {
    throw new Error("fallback fixture missing");
  }

  expect(click(open).defaultPrevented).toBe(true);
  expect(dialog.open).toBe(true);
  expect(click(close).defaultPrevented).toBe(true);
  expect(dialog.open).toBe(false);
});

test("command fallback stays dormant when native invokers are exposed", () => {
  const descriptor = Object.getOwnPropertyDescriptor(
    HTMLButtonElement.prototype,
    "commandForElement",
  );
  Object.defineProperty(HTMLButtonElement.prototype, "commandForElement", {
    configurable: true,
    value: null,
  });
  try {
    root.innerHTML =
      '<button commandfor="dialog" command="show-modal"></button>' +
      '<dialog id="dialog"></dialog>';
    const button = root.querySelector("button");
    const dialog = root.querySelector("dialog");
    if (button === null || dialog === null) throw new Error("fixture missing");
    const show = vi.spyOn(dialog, "showModal");
    expect(click(button).defaultPrevented).toBe(false);
    expect(show).not.toHaveBeenCalled();
  } finally {
    if (descriptor === undefined) {
      Reflect.deleteProperty(HTMLButtonElement.prototype, "commandForElement");
    } else {
      Object.defineProperty(
        HTMLButtonElement.prototype,
        "commandForElement",
        descriptor,
      );
    }
  }
});

test("request-close preserves the cancellable dialog close path", () => {
  root.innerHTML =
    '<button commandfor="dialog" command="request-close"></button>' +
    '<dialog id="dialog" open></dialog>';
  const button = root.querySelector("button");
  const dialog = root.querySelector("dialog");
  if (button === null || dialog === null) throw new Error("fixture missing");
  const requestClose = vi.fn();
  Object.defineProperty(dialog, "requestClose", { value: requestClose });

  expect(click(button).defaultPrevented).toBe(true);
  expect(requestClose).toHaveBeenCalledOnce();
  expect(dialog.open).toBe(true);
});

test("request-close fallback honors a prevented cancel event", () => {
  root.innerHTML =
    '<button commandfor="dialog" command="request-close"></button>' +
    '<dialog id="dialog" open></dialog>';
  const button = root.querySelector("button");
  const dialog = root.querySelector("dialog");
  if (button === null || dialog === null) throw new Error("fixture missing");
  Object.defineProperty(dialog, "requestClose", { value: undefined });
  dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
  });

  expect(click(button).defaultPrevented).toBe(true);
  expect(dialog.open).toBe(true);
});

test("request-close fallback closes after an unopposed cancel event", () => {
  root.innerHTML =
    '<button commandfor="dialog" command="request-close"></button>' +
    '<dialog id="dialog" open></dialog>';
  const button = root.querySelector("button");
  const dialog = root.querySelector("dialog");
  if (button === null || dialog === null) throw new Error("fixture missing");
  Object.defineProperty(dialog, "requestClose", { value: undefined });
  let cancels = 0;
  dialog.addEventListener("cancel", () => {
    cancels += 1;
  });

  expect(click(button).defaultPrevented).toBe(true);
  expect(cancels).toBe(1);
  expect(dialog.open).toBe(false);
});

test("popovertarget falls back to the imperative popover API", () => {
  const descriptor = Object.getOwnPropertyDescriptor(
    HTMLButtonElement.prototype,
    "popoverTargetElement",
  );
  Reflect.deleteProperty(HTMLButtonElement.prototype, "popoverTargetElement");
  try {
    root.innerHTML =
      '<button popovertarget="menu"></button><div id="menu" popover></div>';
    const button = root.querySelector("button");
    const menu = root.querySelector<HTMLElement>("#menu");
    if (button === null || menu === null) throw new Error("fixture missing");
    const show = vi.fn();
    Object.defineProperty(menu, "showPopover", { value: show });

    expect(click(button).defaultPrevented).toBe(true);
    expect(show).toHaveBeenCalledOnce();
  } finally {
    if (descriptor !== undefined) {
      Object.defineProperty(
        HTMLButtonElement.prototype,
        "popoverTargetElement",
        descriptor,
      );
    }
  }
});
