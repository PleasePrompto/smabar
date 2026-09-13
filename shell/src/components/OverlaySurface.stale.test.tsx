// @vitest-environment happy-dom
// @vitest-environment-options { "settings": { "navigation": { "disableChildFrameNavigation": true } } }
import { act } from "react";
import type { Root } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { expect, test, vi } from "vitest";

import { useSmabar } from "../store/bar";
import { recordMemoryUi } from "../ipc/memoryProbe";
import { setEmbedRoot } from "../plugins/embeds";
import {
  installOverlayHarness,
  kitContent,
  systemContent,
  type Listener,
} from "./OverlaySurface.testkit";
import { OverlaySurface } from "./OverlaySurface";

const { listeners, reportMeasureMock, pinMock, invokeMock, takeQueue } =
  vi.hoisted(() => ({
    listeners: new Map<string, Listener>(),
    reportMeasureMock: vi.fn(() => Promise.resolve()),
    pinMock: vi.fn(() => Promise.resolve()),
    invokeMock: vi.fn(),
    takeQueue: [] as unknown[][],
  }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, listener: Listener) => {
    listeners.set(name, listener);
    return Promise.resolve(() => undefined);
  }),
}));

vi.mock("../ipc/overlay", () => ({
  closeFlyoutSurface: vi.fn(() => Promise.resolve()),
  pinFlyoutSurface: pinMock,
  reportFlyoutMeasure: reportMeasureMock,
  reportOverlayPointer: vi.fn(() => Promise.resolve()),
}));

vi.mock("../ipc/call", () => ({ call: vi.fn(() => Promise.resolve()) }));

vi.mock("../ipc/memoryProbe", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../ipc/memoryProbe")>()),
  recordMemoryUi: vi.fn(),
}));

let host: HTMLDivElement;
let root: Root;
const { emit, deliver } = installOverlayHarness(
  { listeners, reportMeasureMock, pinMock, invokeMock, takeQueue },
  (mounted, mountedRoot) => {
    host = mounted;
    root = mountedRoot;
  },
);

test("rejects stale, wrong-tile and closed pushes while counting received traffic", async () => {
  const push = {
    generation: 2,
    pluginId: "systeminfo",
    tileId: "system",
    target: "flyout",
    html: "live",
  };
  await deliver([push]);
  expect(useSmabar.getState().pluginUi).toEqual({});
  emit("surface-flyout", {
    generation: 2,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    content: systemContent,
  });
  await deliver(
    [
      { ...push, generation: 1 },
      { ...push, generation: 3 },
      { ...push, tileId: "other" },
      { ...push, target: "tile" },
      { ...push, target: "popup" },
    ],
    2,
  );
  expect(useSmabar.getState().pluginUi).toEqual({
    "systeminfo/system/flyout": "SystemInfo content",
  });
  await deliver([push]);
  expect(host.querySelector("[data-plugin-id]")?.shadowRoot?.textContent).toBe(
    "live",
  );
  emit("flyout-closed", { generation: 1 });
  expect(useSmabar.getState().pluginUi).toEqual({
    "systeminfo/system/flyout": "live",
  });
  emit("flyout-closed", { generation: 2 });
  await deliver([push]);
  expect(useSmabar.getState().pluginUi).toEqual({});
  expect(host.childElementCount).toBe(0);
  expect(
    vi
      .mocked(recordMemoryUi)
      .mock.calls.filter(([, source]) => source === "live"),
  ).toHaveLength(6);
});

test.each([false, true])(
  "a close tombstone rejects a delayed open (was open: %s)",
  (wasOpen) => {
    const request = {
      generation: 3,
      tileId: "plugin:systeminfo:system",
      mode: "peek",
      content: systemContent,
    };
    if (wasOpen) emit("surface-flyout", { ...request, generation: 2 });
    emit("flyout-closed", { generation: 3 });
    emit("surface-flyout", request);
    emit("surface-flyout", { ...request, generation: 2 });
    emit("flyout-pin-requested", 3);
    expect(host.childElementCount).toBe(0);
    expect(useSmabar.getState().pluginUi).toEqual({});
    expect(pinMock).not.toHaveBeenCalled();
    emit("surface-flyout", { ...request, generation: 4 });
    expect(
      host.querySelector("[data-plugin-id]")?.shadowRoot?.textContent,
    ).toBe("SystemInfo content");
  },
);

test("an embed only starts when pinned and survives an identical active update", async () => {
  setEmbedRoot("http://127.0.0.1:1234/embed");
  const html =
    '<iframe src="https://www.youtube-nocookie.com/embed/example" title="Video"></iframe>';
  emit("surface-flyout", {
    generation: 1,
    tileId: "plugin:systeminfo:system",
    mode: "peek",
    content: { hover: null, flyout: html },
  });
  const shadow = host.querySelector("[data-plugin-id]")?.shadowRoot;
  expect(shadow?.querySelector("iframe")).toBeNull();
  emit("surface-flyout", {
    generation: 1,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    preserveContent: false,
  });
  const frame = shadow?.querySelector("iframe");
  expect(frame).not.toBeNull();
  await deliver([
    {
      generation: 1,
      pluginId: "systeminfo",
      tileId: "system",
      target: "flyout",
      html,
    },
  ]);
  expect(shadow?.querySelector("iframe")).toBe(frame);
});

test("stale placements and same-generation wrong-tile opens leave the current flyout intact", () => {
  const request = {
    generation: 3,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    content: systemContent,
  };
  emit("surface-flyout", request);
  const placement = {
    generation: 3,
    direction: "up",
    pointerX: 10,
    x: 12,
    y: 34,
  };
  emit("overlay-placement", placement);
  emit("surface-flyout", {
    ...request,
    tileId: "plugin:kitshow:kit",
    content: kitContent,
  });
  emit("overlay-placement", { ...placement, generation: 2, x: 99 });
  expect(host.querySelector("[data-plugin-id]")?.shadowRoot?.textContent).toBe(
    "SystemInfo content",
  );
  expect(host.firstElementChild?.getAttribute("style")).toContain("left: 12px");
  expect(host.firstElementChild?.getAttribute("style")).toContain(
    "visibility: visible",
  );
});

test.each([false, true])(
  "readiness waits for every listener, excluding disposed mounts (%s)",
  async (dispose) => {
    const onReady = vi.fn();
    const unlisten = vi.fn();
    let finish: ((stop: () => void) => void) | undefined;
    vi.mocked(listen).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await act(async () => {
      root.render(<OverlaySurface onReady={onReady} />);
      await Promise.resolve();
    });
    expect(onReady).not.toHaveBeenCalled();
    if (dispose)
      act(() => {
        root.render(null);
      });
    await act(async () => {
      finish?.(unlisten);
      await Promise.resolve();
    });
    expect(onReady).toHaveBeenCalledTimes(dispose ? 0 : 1);
    expect(unlisten).toHaveBeenCalledTimes(dispose ? 1 : 0);
  },
);
