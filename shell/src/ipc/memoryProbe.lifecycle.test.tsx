// @vitest-environment happy-dom
import { act, StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { ShadowHost } from "../plugins/PluginContent";
import { beginMemoryHost, initMemoryProbe } from "./memoryProbe";
import { uiLog } from "./log";

vi.mock("./log", () => ({ uiLog: vi.fn(), reportError: vi.fn() }));

let host: HTMLDivElement;
let root: Root | undefined;

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(uiLog).mockClear();
  initMemoryProbe(null, "overlay");
  host = document.createElement("div");
  document.body.append(host);
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  host.remove();
  initMemoryProbe(null, "overlay");
  vi.restoreAllMocks();
  vi.useRealTimers();
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = false;
});

const identity = {
  pluginId: "probe",
  tileId: "clock",
  target: "flyout",
  scope: "flyout",
  generation: 42,
  htmlUnits: 2,
} as const;

function lifecycle(phase: "mounted" | "cleanup") {
  return vi
    .mocked(uiLog)
    .mock.calls.filter((call) => call[1] === `memory probe host ${phase}`)
    .map((call) => call[2]?.fields);
}

test("disabled host diagnostics allocate no tracker or timer and read no clock", () => {
  const now = vi.spyOn(performance, "now");
  expect(beginMemoryHost(identity)).toBeUndefined();
  expect(now).not.toHaveBeenCalled();
  expect(uiLog).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

test("host diagnostics count open shadow trees and balance cleanup exactly once", () => {
  initMemoryProbe("observe", "overlay");
  const shadow = host.attachShadow({ mode: "open" });
  shadow.innerHTML =
    '<form><input><span data-clock-text="">private</span></form><div></div>';
  shadow
    .querySelector("div")
    ?.attachShadow({ mode: "open" })
    .append(document.createElement("input"));
  const probe = beginMemoryHost(identity);
  vi.advanceTimersByTime(12);
  probe?.mounted(shadow, true);
  probe?.mounted(shadow, true);
  expect(lifecycle("mounted")).toHaveLength(1);
  expect(lifecycle("mounted")[0]?.instanceId).toBeTypeOf("number");
  expect(lifecycle("mounted")[0]).toMatchObject({
    ...identity,
    role: "overlay",
    mode: "observe",
    durationMs: 12,
    domElements: 5,
    domInputs: 2,
    domForms: 1,
    domShadowRoots: 2,
    clockElements: 1,
    clockEnhancers: 1,
    activeHosts: 1,
    activeClockEnhancers: 1,
  });
  vi.advanceTimersByTime(8);
  probe?.cleanup();
  probe?.cleanup();
  expect(lifecycle("cleanup")).toHaveLength(1);
  expect(lifecycle("cleanup")[0]).toMatchObject({
    instanceId: lifecycle("mounted")[0]?.instanceId,
    durationMs: 20,
    activeHosts: 0,
    activeClockEnhancers: 0,
  });
  expect(JSON.stringify(vi.mocked(uiLog).mock.calls)).not.toContain("private");
  vi.advanceTimersByTime(31_000);
  expect(vi.mocked(uiLog).mock.calls.at(-1)?.[2]?.fields).toMatchObject({
    hostMounts: 1,
    hostCleanups: 1,
    clockStarts: 1,
    clockStops: 1,
    clockEnhancerStarts: 1,
    clockEnhancerStops: 1,
    activeHosts: 0,
  });
});

test("reinitialization resets gauges and ignores late cleanup from the old session", () => {
  initMemoryProbe("observe", "overlay");
  const shadow = host.attachShadow({ mode: "open" });
  const old = beginMemoryHost(identity);
  old?.mounted(shadow, true);
  initMemoryProbe("no-clocks", "overlay");
  const current = beginMemoryHost(identity);
  current?.mounted(shadow, false);
  old?.cleanup();
  current?.cleanup();
  expect(lifecycle("cleanup")).toHaveLength(1);
  expect(lifecycle("cleanup")[0]).toMatchObject({
    mode: "no-clocks",
    clockElements: 0,
    clockEnhancers: 0,
    activeHosts: 0,
    activeClockEnhancers: 0,
  });
  expect(lifecycle("mounted")[1]?.instanceId).toBeGreaterThan(
    Number(lifecycle("mounted")[0]?.instanceId),
  );
  vi.advanceTimersByTime(31_000);
  expect(vi.mocked(uiLog).mock.calls.at(-1)?.[2]?.fields).toMatchObject({
    hostMounts: 1,
    hostCleanups: 1,
    clockEnhancerStarts: 0,
    clockEnhancerStops: 0,
  });
});

test("StrictMode and changed HTML keep real ShadowHost setups and cleanups balanced", () => {
  initMemoryProbe("observe", "overlay");
  root = createRoot(host);
  const render = (text: string) => {
    act(() => {
      root?.render(
        <StrictMode>
          <ShadowHost
            pluginId="probe"
            tileId="clock"
            memoryScope="flyout"
            probeGeneration={42}
            html={`<form><input type="search"><span data-clock-text="" data-clock-format="unix">${text}</span></form>`}
          />
        </StrictMode>,
      );
    });
  };
  render("first");
  expect(lifecycle("mounted")).toHaveLength(2);
  expect(lifecycle("cleanup")).toHaveLength(1);
  expect(lifecycle("mounted").at(-1)).toMatchObject({
    generation: 42,
    activeHosts: 1,
    activeClockEnhancers: 1,
    domInputs: 1,
    domForms: 1,
  });
  render("😀");
  expect(lifecycle("mounted")).toHaveLength(3);
  expect(lifecycle("cleanup")).toHaveLength(2);
  const markup =
    '<form><input type="search"><span data-clock-text="" data-clock-format="unix">😀</span></form>';
  expect(lifecycle("mounted").at(-1)?.htmlUnits).toBe(markup.length);
  act(() => root?.unmount());
  root = undefined;
  expect(lifecycle("cleanup")).toHaveLength(3);
  expect(lifecycle("cleanup").at(-1)).toMatchObject({
    activeHosts: 0,
    activeClockEnhancers: 0,
  });
  expect(vi.getTimerCount()).toBe(1);
  expect(
    vi.mocked(uiLog).mock.calls.every((call) => call[2]?.deduplicate === false),
  ).toBe(true);
});
