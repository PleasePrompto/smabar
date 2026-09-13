// @vitest-environment happy-dom
// @vitest-environment-options { "settings": { "navigation": { "disableChildFrameNavigation": true } } }
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { registerTile, unregisterPluginTiles } from "./registry";
import { call } from "../ipc/call";
import { useSmabar } from "../store/bar";
import { recordMemoryUi } from "../ipc/memoryProbe";
import { setEmbedRoot } from "../plugins/embeds";
import { OverlaySurface } from "./OverlaySurface";

type Listener = (event: { payload: unknown }) => void;

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

const systemContent = { hover: null, flyout: "SystemInfo content" };
const kitContent = { hover: null, flyout: "UI Kit content" };

let host: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  listeners.clear();
  reportMeasureMock.mockClear();
  pinMock.mockClear();
  takeQueue.length = 0;
  invokeMock.mockReset();
  invokeMock.mockImplementation((command: string) =>
    command === "take_plugin_ui"
      ? Promise.resolve(takeQueue.shift() ?? [])
      : Promise.reject(new Error(`unexpected command: ${command}`)),
  );
  useSmabar.setState(useSmabar.getInitialState(), true);
  vi.mocked(recordMemoryUi).mockClear();
  registerTile({
    id: "plugin:systeminfo:system",
    pluginId: "systeminfo",
    tile: { id: "system", name: "System", hasFlyout: true },
    meta: { name: "System" },
  });
  registerTile({
    id: "plugin:kitshow:kit",
    pluginId: "kitshow",
    tile: { id: "kit", name: "UI Kit", hasFlyout: true },
    meta: { name: "UI Kit" },
  });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(<OverlaySurface />);
    await Promise.resolve();
  });
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
  setEmbedRoot("");
  unregisterPluginTiles("systeminfo");
  unregisterPluginTiles("kitshow");
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = false;
});

test("ignores UI pushes from plugins outside the open flyout", async () => {
  emit("surface-flyout", {
    generation: 1,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    content: systemContent,
  });
  const initialMeasures = reportMeasureMock.mock.calls.length;

  await deliver([
    {
      generation: 1,
      pluginId: "crypto",
      tileId: "crypto",
      target: "flyout",
      html: "unrelated",
    },
  ]);
  expect(reportMeasureMock).toHaveBeenCalledTimes(initialMeasures);

  await deliver([
    {
      generation: 1,
      pluginId: "systeminfo",
      tileId: "system",
      target: "flyout",
      html: "Updated system content",
    },
  ]);
  // The open flyout takes the push — but its box did not change, so the
  // native place/reveal chain is not re-run for it.
  expect(host.querySelector("[data-plugin-id]")?.shadowRoot?.textContent).toBe(
    "Updated system content",
  );
  expect(reportMeasureMock).toHaveBeenCalledTimes(initialMeasures);
});

test.each([undefined, "SystemInfo content"])(
  "pins identical content in place (hover: %s)",
  (hover) => {
    emit("surface-flyout", {
      generation: 1,
      tileId: "plugin:systeminfo:system",
      mode: "peek",
      content: { ...systemContent, hover: hover ?? null },
    });
    const peekMeasures = reportMeasureMock.mock.calls.length;
    const content =
      host.querySelector("[data-plugin-id]")?.shadowRoot?.firstElementChild;
    const scroller = host.querySelector(".surface-scroll");
    if (scroller !== null) scroller.scrollTop = 42;

    // No native staging: the already visible DOM must stay put and interactive.
    emit("surface-flyout", {
      generation: 1,
      tileId: "plugin:systeminfo:system",
      mode: "pinned",
      preserveContent: true,
    });

    expect(reportMeasureMock).toHaveBeenCalledTimes(peekMeasures);
    expect(
      host.querySelector("[data-plugin-id]")?.shadowRoot?.firstElementChild,
    ).toBe(content);
    expect(scroller?.hasAttribute("data-settled")).toBe(true);
    expect(scroller?.hasAttribute("data-sb-suppress-click")).toBe(false);
    expect(scroller?.scrollTop).toBe(42);
  },
);

test.each([
  { hover: undefined, full: "Full", replace: false },
  { hover: "Full", full: "Full", replace: false },
  { hover: "Compact", full: "Full", replace: true },
  { hover: "Only preview", full: undefined, replace: false },
  {
    hover: undefined,
    full: '<iframe src="https://example.com/player"></iframe>',
    replace: true,
  },
])(
  "both pin entry points compare the live content: %j",
  ({ hover, full, replace }) => {
    emit("surface-flyout", {
      generation: 7,
      tileId: "plugin:systeminfo:system",
      mode: "peek",
      content: { hover: hover ?? null, flyout: full ?? null },
    });
    emit("flyout-pin-requested", 6);
    expect(pinMock).not.toHaveBeenCalled();
    emit("flyout-pin-requested", 7);
    expect(pinMock).toHaveBeenLastCalledWith(7, replace);
    pinMock.mockClear();
    act(() => {
      host.querySelector<HTMLElement>('[data-capture="flyout"]')?.click();
    });
    expect(pinMock).toHaveBeenCalledExactlyOnceWith(7, replace);
  },
);

test("replaces a compact preview and re-measures even an equally sized full view", () => {
  emit("surface-flyout", {
    generation: 1,
    tileId: "plugin:systeminfo:system",
    mode: "peek",
    content: { ...systemContent, hover: "Compact" },
  });
  const preview =
    host.querySelector("[data-plugin-id]")?.shadowRoot?.firstElementChild;
  const measures = reportMeasureMock.mock.calls.length;
  emit("surface-flyout", {
    generation: 1,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    preserveContent: false,
  });
  const shadow = host.querySelector("[data-plugin-id]")?.shadowRoot;
  expect(shadow?.firstElementChild).not.toBe(preview);
  expect(shadow?.textContent).toBe("SystemInfo content");
  expect(reportMeasureMock).toHaveBeenCalledTimes(measures + 1);
  expect(
    host
      .querySelector(".surface-scroll")
      ?.hasAttribute("data-sb-suppress-click"),
  ).toBe(false);
});

test("a preview stays hoverable, and a click on its content pins instead of acting", () => {
  emit("surface-flyout", {
    generation: 3,
    tileId: "plugin:systeminfo:system",
    mode: "peek",
    content: {
      ...systemContent,
      hover: '<button data-action="mute" title="Mute">Mute</button>',
    },
  });
  const scroller = host.querySelector(".surface-scroll");
  expect(scroller?.hasAttribute("inert")).toBe(false);
  expect(scroller?.hasAttribute("data-sb-suppress-click")).toBe(true);
  const button = host
    .querySelector("[data-plugin-id]")
    ?.shadowRoot?.querySelector("button");
  expect(button?.dataset.sbTooltip).toBe("Mute");
  let allowed = true;
  act(() => {
    allowed =
      button?.dispatchEvent(
        new MouseEvent("click", {
          bubbles: true,
          composed: true,
          cancelable: true,
        }),
      ) ?? true;
  });
  expect(allowed).toBe(false);
  expect(pinMock).toHaveBeenCalledExactlyOnceWith(3, true);
  expect(vi.mocked(call)).not.toHaveBeenCalledWith(
    "plugin_action",
    expect.anything(),
  );
});

test("does not reuse a plugin shadow host for another tile", () => {
  emit("surface-flyout", {
    generation: 1,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    content: systemContent,
  });
  const systemHost = host.querySelector("[data-plugin-id]");

  emit("surface-flyout", {
    generation: 2,
    tileId: "plugin:kitshow:kit",
    mode: "pinned",
    content: kitContent,
  });
  const kitHost = host.querySelector("[data-plugin-id]");

  expect(kitHost).not.toBe(systemHost);
  expect(kitHost?.shadowRoot?.textContent).toBe("UI Kit content");
});

test("ignores a flyout request delivered after its replacement", () => {
  emit("surface-flyout", {
    generation: 3,
    tileId: "plugin:kitshow:kit",
    mode: "pinned",
    content: kitContent,
  });
  // Concurrent staging in the core can deliver the older request late.
  emit("surface-flyout", {
    generation: 2,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    content: systemContent,
  });

  expect(host.querySelector("[data-plugin-id]")?.shadowRoot?.textContent).toBe(
    "UI Kit content",
  );
});

test("keeps only the active tile's one-shot content, including empty HTML", () => {
  emit("surface-flyout", {
    generation: 1,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    content: { hover: "preview", flyout: "full" },
  });
  emit("surface-flyout", {
    generation: 2,
    tileId: "plugin:kitshow:kit",
    mode: "peek",
    content: { hover: "", flyout: null },
  });
  expect(useSmabar.getState().pluginUi).toEqual({ "kitshow/kit/hover": "" });
  expect(host.querySelector("[data-plugin-id]")?.shadowRoot?.textContent).toBe(
    "",
  );
  expect(recordMemoryUi).toHaveBeenCalledWith(0, "snapshot");
  emit("surface-flyout", {
    generation: 2,
    tileId: "plugin:kitshow:kit",
    mode: "pinned",
    content: { hover: null, flyout: "recovered" },
  });
  expect(useSmabar.getState().pluginUi).toEqual({
    "kitshow/kit/flyout": "recovered",
  });
});

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

function emit(name: string, payload: unknown): void {
  const listener = listeners.get(name);
  if (listener === undefined) throw new Error(`missing ${name} listener`);
  act(() => {
    listener({ payload });
  });
}

/** Signals `generation`; the pull for it answers with `rendered`. */
type Pushed = { generation: number } & Record<string, unknown>;

async function deliver(
  rendered: Pushed[],
  generation = rendered[0]?.generation,
): Promise<void> {
  takeQueue.length = 0;
  takeQueue.push(rendered);
  const listener = listeners.get("plugin-ui-overlay");
  if (listener === undefined) throw new Error("missing overlay listener");
  await act(async () => {
    listener({ payload: { generation } });
    for (let turn = 0; turn < 8; turn += 1) await Promise.resolve();
  });
}
