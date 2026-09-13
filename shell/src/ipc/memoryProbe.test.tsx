// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { ShadowHost } from "../plugins/PluginContent";
import { OverlaySurface } from "../components/OverlaySurface";
import { registerTile, unregisterPluginTiles } from "../components/registry";
import { useSmabar } from "../store/bar";
import {
  initMemoryProbe,
  recordMemoryBatch,
  recordMemoryDomCommit,
  recordMemoryUi,
  suppressMemoryStateUpdate,
} from "./memoryProbe";
import { uiLog } from "./log";

type Listener = (event: { payload: unknown }) => void;
const { listeners, invokeMock, takeQueue } = vi.hoisted(() => ({
  listeners: new Map<string, Listener>(),
  invokeMock: vi.fn(),
  takeQueue: [] as unknown[][],
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, listener: Listener) => {
    listeners.set(name, listener);
    return Promise.resolve(() => {
      listeners.delete(name);
    });
  }),
}));

vi.mock("./overlay", () => ({
  closeFlyoutSurface: vi.fn(() => Promise.resolve()),
  pinFlyoutSurface: vi.fn(() => Promise.resolve()),
  reportFlyoutMeasure: vi.fn(() => Promise.resolve()),
  reportOverlayPointer: vi.fn(() => Promise.resolve()),
}));

vi.mock("./log", () => ({
  uiLog: vi.fn(),
  reportError: vi.fn(),
}));

let reactRoot: Root | undefined;
let host: HTMLDivElement;

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-09-12T12:00:00Z"));
  vi.mocked(uiLog).mockClear();
  listeners.clear();
  initMemoryProbe(null, "bar");
  host = document.createElement("div");
  document.body.append(host);
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(() => {
  act(() => {
    reactRoot?.unmount();
  });
  reactRoot = undefined;
  host.remove();
  unregisterPluginTiles("probe");
  initMemoryProbe(null, "bar");
  vi.restoreAllMocks();
  vi.useRealTimers();
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = false;
});

test("ordinary launches start no sampler; diagnostics report bounded interval totals", () => {
  recordMemoryUi(90, "live");
  recordMemoryDomCommit(90);
  expect(vi.getTimerCount()).toBe(0);
  vi.advanceTimersByTime(31_000);
  expect(uiLog).not.toHaveBeenCalled();

  initMemoryProbe("observe", "overlay");
  host.attachShadow({ mode: "open" }).innerHTML =
    "<span>private payload</span>";
  recordMemoryBatch();
  recordMemoryUi(2, "live");
  recordMemoryUi(7, "snapshot");
  recordMemoryDomCommit(7);
  vi.advanceTimersByTime(31_000);
  const sample = vi.mocked(uiLog).mock.calls.at(-1);
  expect(sample?.slice(0, 2)).toEqual(["info", "memory probe shell sample"]);
  expect(sample?.[2]?.fields?.session).toBeTypeOf("number");
  expect(sample?.[2]?.fields).toMatchObject({
    role: "overlay",
    mode: "observe",
    elapsedMs: 31_000,
    liveBatches: 1,
    liveEvents: 1,
    liveHtmlUnits: 2,
    snapshotEvents: 1,
    snapshotHtmlUnits: 7,
    domCommits: 1,
    committedHtmlUnits: 7,
    suppressedStateUpdates: 0,
    suppressedDomUpdates: 0,
    clockStarts: 0,
    clockStops: 0,
    clockEnhancerStarts: 0,
    clockEnhancerStops: 0,
    hostMounts: 0,
    hostCleanups: 0,
    activeHosts: 0,
    activeClockEnhancers: 0,
    domElements: document.querySelectorAll("*").length + 1,
    domInputs: 0,
    domForms: 0,
    domShadowRoots: 1,
    visibility: document.visibilityState,
  });
  vi.advanceTimersByTime(31_000);
  expect(vi.mocked(uiLog).mock.calls.at(-1)?.[2]?.fields).toMatchObject({
    liveEvents: 0,
    domCommits: 0,
  });
  initMemoryProbe("no-events", "bar");
  expect(vi.getTimerCount()).toBe(1);
  initMemoryProbe(undefined, "bar");
  expect(vi.getTimerCount()).toBe(0);
});

test.each([null, "observe", "no-state", "no-dom", "no-clocks"] as const)(
  "%s keeps the intended DOM and clock paths separate",
  (mode) => {
    initMemoryProbe(mode, "bar");
    reactRoot = createRoot(host);
    const render = (label: string) => {
      act(() => {
        reactRoot?.render(
          <ShadowHost
            pluginId="probe"
            tileId="main"
            html={`<b>${label}</b><span data-clock-text="" data-clock-format="unix">clock fallback</span>`}
          />,
        );
      });
    };
    render("first");
    const shadow = host.firstElementChild?.shadowRoot;
    const clock = shadow?.querySelector("span");
    expect(clock?.textContent).toBe(
      mode === "no-clocks" ? "clock fallback" : "1789214400",
    );
    render("second");
    expect(shadow?.querySelector("b")?.textContent).toBe(
      mode === "no-dom" ? "first" : "second",
    );
    act(() => {
      vi.advanceTimersByTime(2_000);
    });
    expect(shadow?.querySelector("span")?.textContent).toBe(
      mode === "no-clocks" ? "clock fallback" : "1789214402",
    );
    if (mode === "no-dom") {
      expect(shadow?.querySelector("span")).toBe(clock);
    }
    act(() => {
      vi.advanceTimersByTime(29_000);
    });
    if (mode !== null) {
      expect(vi.mocked(uiLog).mock.calls.at(-1)?.[2]?.fields).toMatchObject({
        domCommits: mode === "no-dom" ? 1 : 2,
        suppressedDomUpdates: mode === "no-dom" ? 1 : 0,
        clockStarts: mode === "no-clocks" ? 0 : mode === "no-dom" ? 1 : 2,
        clockEnhancerStarts:
          mode === "no-clocks" ? 0 : mode === "no-dom" ? 1 : 2,
        activeClockEnhancers: mode === "no-clocks" ? 0 : 1,
        activeHosts: 1,
      });
    }
  },
);

test("only no-state freezes after its own 30-second warmup and resets on reinitialization", () => {
  for (const mode of [
    null,
    "observe",
    "no-events",
    "no-dom",
    "no-clocks",
  ] as const) {
    initMemoryProbe(mode, "bar");
    vi.advanceTimersByTime(30_000);
    expect(suppressMemoryStateUpdate()).toBe(false);
  }
  initMemoryProbe("no-state", "bar");
  expect(suppressMemoryStateUpdate()).toBe(false);
  vi.advanceTimersByTime(29_999);
  expect(suppressMemoryStateUpdate()).toBe(false);
  vi.advanceTimersByTime(1);
  expect(suppressMemoryStateUpdate()).toBe(true);
  vi.advanceTimersByTime(1_000);
  expect(vi.mocked(uiLog).mock.calls.at(-1)?.[2]?.fields).toMatchObject({
    suppressedStateUpdates: 1,
  });
  vi.advanceTimersByTime(31_000);
  expect(vi.mocked(uiLog).mock.calls.at(-1)?.[2]?.fields).toMatchObject({
    suppressedStateUpdates: 0,
  });
  initMemoryProbe("no-state", "overlay");
  expect(suppressMemoryStateUpdate()).toBe(false);
  expect(vi.getTimerCount()).toBe(1);
  initMemoryProbe(null, "overlay");
  expect(suppressMemoryStateUpdate()).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
});

test("no-state receives open flyout updates without state work, while snapshots and cleanup remain live", async () => {
  initMemoryProbe("no-state", "overlay");
  useSmabar.setState(useSmabar.getInitialState(), true);
  registerTile({
    id: "plugin:probe:main",
    pluginId: "probe",
    tile: { id: "main", name: "Probe", hasFlyout: true },
    meta: { name: "Probe" },
  });
  reactRoot = createRoot(host);
  await act(async () => {
    reactRoot?.render(<OverlaySurface />);
    await Promise.resolve();
  });
  const emit = (name: string, payload: unknown) => {
    const listener = listeners.get(name);
    if (listener === undefined) throw new Error(`missing ${name} listener`);
    act(() => {
      listener({ payload });
    });
  };
  const request = {
    generation: 1,
    tileId: "plugin:probe:main",
    mode: "pinned",
    content: { hover: null, flyout: "snapshot" },
  };
  const live = {
    generation: 1,
    pluginId: "probe",
    tileId: "main",
    target: "flyout",
    html: "warmup",
  };
  invokeMock.mockImplementation((command: string) =>
    command === "take_plugin_ui"
      ? Promise.resolve(takeQueue.shift() ?? [])
      : Promise.reject(new Error(`unexpected command: ${command}`)),
  );
  const push = async (rendered: unknown[]) => {
    takeQueue.push(rendered);
    await act(async () => {
      emit("plugin-ui-overlay", { generation: 1 });
      for (let turn = 0; turn < 8; turn += 1) await Promise.resolve();
    });
  };
  emit("surface-flyout", request);
  expect(vi.mocked(uiLog).mock.calls.at(-1)?.[2]?.fields).toMatchObject({
    pluginId: "probe",
    tileId: "main",
    generation: 1,
    scope: "flyout",
  });
  await push([live]);
  expect(useSmabar.getState().pluginUi["probe/main/flyout"]).toBe("warmup");
  const content =
    host.querySelector("[data-plugin-id]")?.shadowRoot?.firstChild;
  act(() => {
    vi.advanceTimersByTime(30_000);
  });
  const getState = vi.spyOn(useSmabar, "getState");
  await push([{ ...live, html: "blocked" }]);
  expect(getState).not.toHaveBeenCalled();
  getState.mockRestore();
  expect(useSmabar.getState().pluginUi["probe/main/flyout"]).toBe("warmup");
  expect(host.querySelector("[data-plugin-id]")?.shadowRoot?.firstChild).toBe(
    content,
  );
  act(() => {
    vi.advanceTimersByTime(1_000);
  });
  expect(vi.mocked(uiLog).mock.calls.at(-1)?.[2]?.fields).toMatchObject({
    liveBatches: 2,
    liveEvents: 2,
    liveHtmlUnits: 13,
    snapshotEvents: 1,
    snapshotHtmlUnits: 8,
    suppressedStateUpdates: 1,
    domCommits: 2,
  });
  emit("flyout-closed", request);
  expect(useSmabar.getState().pluginUi).toEqual({});
  expect(host.childElementCount).toBe(0);
  emit("surface-flyout", {
    ...request,
    generation: 2,
    content: { hover: null, flyout: "fresh snapshot" },
  });
  expect(host.querySelector("[data-plugin-id]")?.shadowRoot?.textContent).toBe(
    "fresh snapshot",
  );
  act(() => {
    reactRoot?.unmount();
  });
  reactRoot = undefined;
  expect(listeners.size).toBe(0);
  expect(vi.getTimerCount()).toBe(1);
  initMemoryProbe(null, "overlay");
  expect(vi.getTimerCount()).toBe(0);
});
