// @vitest-environment happy-dom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../store/bar";
import { PluginPopup } from "./PluginPopup";
import { Toast } from "./Toast";

const { stageNotificationUpdateMock, invokeMock, callMock } = vi.hoisted(
  () => ({
    stageNotificationUpdateMock: vi.fn<() => Promise<void>>(),
    invokeMock: vi.fn<() => Promise<void>>(),
    callMock: vi.fn<() => Promise<void>>(),
  }),
);

vi.mock("../ipc/surface", () => ({
  stageNotificationUpdate: stageNotificationUpdateMock,
}));
vi.mock("../ipc/log", () => ({ reportError: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("../ipc/call", () => ({ call: callMock }));

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  useSmabar.setState(useSmabar.getInitialState(), true);
  stageNotificationUpdateMock.mockReset();
  stageNotificationUpdateMock.mockResolvedValue();
  invokeMock.mockReset().mockResolvedValue();
  callMock.mockReset().mockResolvedValue();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
  vi.useRealTimers();
});

test("auto-dismiss pauses across hover and keyboard focus", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(1_000);
  act(() => {
    useSmabar.getState().enqueuePopup(
      {
        pluginId: "weather",
        tileId: "weather",
        html: "<button>Details</button>",
        ttlMs: 1_000,
      },
      Date.now(),
    );
    root.render(createElement(PluginPopup));
  });
  const popup = document.querySelector<HTMLElement>('[data-capture="popup"]');
  const button = popup?.querySelector<HTMLButtonElement>("button");
  expect(popup).not.toBeNull();
  expect(button).not.toBeNull();

  await act(async () => {
    await vi.advanceTimersByTimeAsync(600);
    popup?.dispatchEvent(new PointerEvent("pointerover", { bubbles: true }));
    button?.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    await vi.advanceTimersByTimeAsync(5_000);
  });
  expect(useSmabar.getState().popupQueue.visible).toHaveLength(1);

  await act(async () => {
    popup?.dispatchEvent(new PointerEvent("pointerout", { bubbles: true }));
    await vi.advanceTimersByTimeAsync(5_000);
  });
  expect(useSmabar.getState().popupQueue.visible).toHaveLength(1);

  await act(async () => {
    button?.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    await vi.advanceTimersByTimeAsync(399);
  });
  expect(useSmabar.getState().popupQueue.visible).toHaveLength(1);

  await act(async () => {
    await vi.advanceTimersByTimeAsync(1);
  });
  expect(stageNotificationUpdateMock).toHaveBeenCalledOnce();
  expect(useSmabar.getState().popupQueue.visible).toEqual([]);
});

test("notifications use the readable bar surface without edge fading", () => {
  act(() => {
    const state = useSmabar.getState();
    state.setAppearance({ ...state.appearance, barChrome: "flat" });
    state.enqueuePopup({
      pluginId: "weather",
      tileId: "weather",
      html: "<p>Storm warning</p>",
    });
    state.setNotice("popup.dismiss");
    root.render(
      createElement(
        "div",
        null,
        createElement(PluginPopup),
        createElement(Toast),
      ),
    );
  });

  const pluginPopup = document.querySelector('[data-capture="popup"]');
  expect(pluginPopup?.classList.contains("surface-bar")).toBe(false);
  expect(pluginPopup?.classList.contains("surface-flyout")).toBe(true);
  const scroller = pluginPopup?.querySelector(".sb-scroll");
  expect(scroller).not.toBeNull();
  expect(scroller?.classList.contains("surface-scroll")).toBe(false);
  expect(scroller?.getAttribute("style")).toContain("--sb-work-area-height");
  expect(pluginPopup?.getAttribute("style")).not.toContain("animation");

  const shellToast = document.querySelector("[data-shell-toast]");
  expect(shellToast?.classList.contains("surface-flyout")).toBe(true);
});

test("dismissal conceals the native notification surface before removing content", async () => {
  let finishStage: (() => void) | undefined;
  stageNotificationUpdateMock.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finishStage = resolve;
      }),
  );
  act(() => {
    useSmabar.getState().enqueuePopup({
      pluginId: "weather",
      tileId: "weather",
      html: "<p>Storm warning</p>",
    });
    root.render(createElement(PluginPopup));
  });

  const dismiss = document.querySelector<HTMLButtonElement>(
    'button[aria-label="Dismiss notification"]',
  );
  expect(dismiss).not.toBeNull();
  act(() => {
    dismiss?.click();
  });

  expect(stageNotificationUpdateMock).toHaveBeenCalledOnce();
  expect(useSmabar.getState().popupQueue.visible).toHaveLength(1);
  await act(async () => {
    finishStage?.();
    await Promise.resolve();
  });
  expect(useSmabar.getState().popupQueue.visible).toEqual([]);
});

test("managed toast buttons retain their instance and X reports dismissal", async () => {
  act(() => {
    useSmabar.getState().enqueuePopup({
      pluginId: "todos",
      tileId: "todos",
      instanceId: 42,
      html: '<button data-action="complete" data-value="task">Done</button>',
    });
    root.render(createElement(PluginPopup));
  });
  const shadowHost = document.querySelector('[data-plugin-id="todos"]');
  const button = shadowHost?.shadowRoot?.querySelector("button");
  expect(button).toBeInstanceOf(HTMLButtonElement);
  expect(button?.dataset.action).toBe("complete");
  if (!shadowHost || !button) throw new Error("missing popup button");
  // happy-dom does not retarget Shadow DOM events for React like the native webview.
  const click = new MouseEvent("click", { bubbles: true, composed: true });
  const path: EventTarget[] = [button];
  for (
    let node: Node | null = shadowHost;
    node !== null;
    node = node.parentNode
  )
    path.push(node);
  path.push(window);
  vi.spyOn(click, "composedPath").mockReturnValue(path);
  act(() => {
    shadowHost.dispatchEvent(click);
  });
  expect(callMock).toHaveBeenCalledWith("plugin_action", {
    pluginId: "todos",
    tileId: "todos",
    action: "complete",
    value: "task",
    popupInstanceId: 42,
  });
  expect(useSmabar.getState().popupQueue.visible).toHaveLength(1);
  await act(async () => {
    document
      .querySelector<HTMLButtonElement>(
        'button[aria-label="Dismiss notification"]',
      )
      ?.click();
    await Promise.resolve();
  });
  expect(invokeMock).toHaveBeenCalledWith("popup_event_report", {
    instanceId: 42,
    state: "dismissed",
  });
  expect(useSmabar.getState().popupQueue.visible).toEqual([]);
});
