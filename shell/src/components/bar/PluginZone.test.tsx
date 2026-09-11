// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../../store/bar";
import * as log from "../../ipc/log";
import { registerTile, unregisterPluginTiles } from "../registry";
import { LONG_PRESS_MS } from "../dragReorder";
import { PluginZone } from "./PluginZone";

const { callMock, invokeMock } = vi.hoisted(() => ({
  callMock: vi.fn(() => Promise.resolve()),
  invokeMock: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../ipc/call", () => ({ call: callMock }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const pluginId = "reorder-test";
const id = (name: string) => `plugin:${pluginId}:${name}`;
const original = ["a", "hidden", "b", "c"].map(id);
const moved = ["b", "hidden", "c", "a"].map(id);
const frames = new Map<number, FrameRequestCallback>();
let nextFrame: number;
let container: HTMLDivElement;
let root: Root;
let zone: HTMLDivElement;

function pointer(type: string, target: Element, x: number): void {
  act(() => {
    target.dispatchEvent(
      new PointerEvent(type, {
        pointerId: 1,
        isPrimary: true,
        button: 0,
        buttons: type === "pointerup" ? 0 : 1,
        clientX: x,
        clientY: 20,
        bubbles: true,
        composed: true,
      }),
    );
  });
}

function flushFrame(): void {
  act(() => {
    const pending = [...frames.values()];
    frames.clear();
    for (const callback of pending) callback(0);
  });
}

function startDrag(index = 0): void {
  const host = zone.children[index]?.querySelector("[data-plugin-id]");
  if (host === null || host === undefined) {
    throw new Error("missing tile host");
  }
  // Browsers retarget shadow-tree events to this host; happy-dom does not.
  pointer("pointerdown", host, index * 110 + 50);
  act(() => {
    vi.advanceTimersByTime(LONG_PRESS_MS);
  });
  expect(useSmabar.getState().reordering).toBe(true);
}

function renderedOrder(): (string | undefined)[] {
  return [...zone.querySelectorAll<HTMLElement>("button[data-tile-id]")].map(
    (tile) => tile.dataset.tileId,
  );
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  vi.useFakeTimers();
  callMock.mockReset().mockResolvedValue(undefined);
  invokeMock.mockReset().mockResolvedValue(undefined);
  nextFrame = 0;
  frames.clear();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    nextFrame += 1;
    frames.set(nextFrame, callback);
    return nextFrame;
  });
  vi.stubGlobal("cancelAnimationFrame", (handle: number) => {
    frames.delete(handle);
  });
  useSmabar.setState(
    {
      ...useSmabar.getInitialState(),
      pluginOrder: original,
      pluginsHidden: [id("hidden")],
    },
    true,
  );
  for (const name of ["a", "hidden", "b", "c"]) {
    registerTile({
      id: id(name),
      pluginId,
      tile: { id: name, name, hasFlyout: true },
      meta: { name },
    });
    useSmabar
      .getState()
      .setPluginUi(`${pluginId}/${name}/tile`, `<span>${name}</span>`);
  }
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  act(() => {
    root.render(<PluginZone />);
  });
  const found = container.querySelector<HTMLDivElement>("[data-plugin-zone]");
  if (found === null) throw new Error("missing tile zone");
  zone = found;
  // happy-dom has no layout or native pointer capture.
  zone.getBoundingClientRect = () => new DOMRect(0, 0, 320, 40);
  Object.defineProperties(zone, {
    scrollWidth: { value: 320 },
    clientWidth: { value: 320 },
  });
  [...zone.children].forEach((child, index) => {
    child.getBoundingClientRect = () => new DOMRect(index * 110, 0, 100, 40);
  });
  const captured = new Set<number>();
  zone.setPointerCapture = (pointerId) => captured.add(pointerId);
  zone.hasPointerCapture = (pointerId) => captured.has(pointerId);
  zone.releasePointerCapture = (pointerId) => {
    captured.delete(pointerId);
    zone.dispatchEvent(
      new PointerEvent("lostpointercapture", { pointerId, bubbles: true }),
    );
  };
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  unregisterPluginTiles(pluginId);
  useSmabar.setState(useSmabar.getInitialState(), true);
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

test.each([true, false])(
  "dropping persists the visible order and preserves hidden slots (painted: %s)",
  (painted) => {
    startDrag();
    pointer("pointermove", zone, 280);
    if (painted) flushFrame();
    pointer("pointerup", zone, 280);

    expect(callMock).toHaveBeenCalledExactlyOnceWith("update_config", {
      path: "pluginOrder",
      value: moved,
    });
    expect(useSmabar.getState().pluginOrder).toEqual(moved);
    expect(renderedOrder()).toEqual(["b", "c", "a"].map(id));
    expect(useSmabar.getState().reordering).toBe(false);
    expect(document.querySelector(".zone-drop-marker")).toBeNull();
    expect(frames.size).toBe(0);
    expect(
      [...zone.children].every(
        (child) =>
          child instanceof HTMLElement &&
          child.style.getPropertyValue("translate") === "",
      ),
    ).toBe(true);
  },
);

test("the release position wins over the last painted position", () => {
  startDrag();
  pointer("pointermove", zone, 170);
  flushFrame();
  pointer("pointerup", zone, 280);

  expect(callMock).toHaveBeenCalledExactlyOnceWith("update_config", {
    path: "pluginOrder",
    value: moved,
  });
});

test("the last tile can be dropped before the first without a paint", () => {
  startDrag(2);
  pointer("pointermove", zone, -10);
  pointer("pointerup", zone, -10);

  expect(callMock).toHaveBeenCalledExactlyOnceWith("update_config", {
    path: "pluginOrder",
    value: ["c", "hidden", "a", "b"].map(id),
  });
  expect(renderedOrder()).toEqual(["c", "a", "b"].map(id));
});

test("returning to the original slot before release does not save an old preview", () => {
  startDrag();
  pointer("pointermove", zone, 280);
  flushFrame();
  pointer("pointermove", zone, 50);
  pointer("pointerup", zone, 50);

  expect(callMock).not.toHaveBeenCalled();
  expect(useSmabar.getState().pluginOrder).toEqual(original);
  expect(renderedOrder()).toEqual(["a", "b", "c"].map(id));
  expect(frames.size).toBe(0);
});

test.each(["Escape", "pointercancel", "lostpointercapture", "unmount"])(
  "%s cancels a pending move without saving it",
  (reason) => {
    startDrag();
    pointer("pointermove", zone, 280);
    if (reason === "Escape") {
      act(() => {
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      });
    } else if (reason === "unmount") {
      act(() => {
        root.render(null);
      });
    } else {
      pointer(reason, zone, 280);
    }
    flushFrame();

    expect(callMock).not.toHaveBeenCalled();
    expect(useSmabar.getState().pluginOrder).toEqual(original);
    expect(useSmabar.getState().reordering).toBe(false);
    expect(document.querySelector(".zone-drop-marker")).toBeNull();
    expect(frames.size).toBe(0);
  },
);

test("a rejected save restores the previous order and reports the error", async () => {
  const error = new Error("config could not be saved");
  const report = vi
    .spyOn(log, "reportError")
    .mockImplementation(() => undefined);
  callMock.mockRejectedValueOnce(error);
  startDrag();
  pointer("pointermove", zone, 280);
  pointer("pointerup", zone, 280);
  expect(useSmabar.getState().pluginOrder).toEqual(moved);
  await act(async () => {
    await Promise.resolve();
  });

  expect(useSmabar.getState().pluginOrder).toEqual(original);
  expect(renderedOrder()).toEqual(["a", "b", "c"].map(id));
  expect(report).toHaveBeenCalledExactlyOnceWith(error);
});

test.each(["peek", "pinned"])(
  "drag start closes an existing %s in both the bar and native overlay",
  async (mode) => {
    const store = useSmabar.getState();
    const rect = { left: 10, top: 10, width: 100, height: 40 };
    act(() => {
      if (mode === "peek") store.peekFlyout(id("a"), rect, true);
      else store.openPinnedFlyout(id("a"), rect);
    });
    expect(useSmabar.getState().openFlyout).toBe(id("a"));
    startDrag();
    await act(async () => {
      await Promise.resolve();
    });

    expect(useSmabar.getState().openFlyout).toBeNull();
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith("close_flyout", {
      generation: undefined,
    });
  },
);

function hover(index: number): void {
  const tile = zone.children[index]?.querySelector("button");
  if (tile === null || tile === undefined)
    throw new Error("missing hover tile");
  tile.getBoundingClientRect = () => new DOMRect(index * 110, 10, 100, 40);
  act(() => {
    tile.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
  });
}

function enableHover(delayMs: number): void {
  const store = useSmabar.getState();
  act(() => {
    store.setEffects({
      ...store.effects,
      hoverPeek: { enabled: true, delayMs },
    });
  });
}

test("hovering other tiles during a drag cannot open a native flyout", async () => {
  enableHover(100);
  startDrag();
  hover(1);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(200);
  });

  expect(useSmabar.getState().openFlyout).toBeNull();
  expect(invokeMock).not.toHaveBeenCalledWith("open_flyout", expect.anything());
  pointer("pointerup", zone, 50);
  hover(2);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(200);
  });
  expect(useSmabar.getState().openFlyout).toBe(id("c"));
  expect(invokeMock).toHaveBeenCalledWith(
    "open_flyout",
    expect.objectContaining({ tileId: id("c"), mode: "peek" }),
  );
});

test.each(["before", "during"])(
  "a hover timer armed %s a drag cannot open a flyout after a quick drop",
  async (when) => {
    enableHover(500);
    if (when === "before") hover(1);
    startDrag();
    if (when === "during") hover(1);
    pointer("pointerup", zone, 50);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });

    expect(useSmabar.getState().openFlyout).toBeNull();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "open_flyout",
      expect.anything(),
    );
  },
);
