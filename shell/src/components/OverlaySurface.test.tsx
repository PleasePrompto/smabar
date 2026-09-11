// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { registerTile, unregisterPluginTiles } from "./registry";
import { call } from "../ipc/call";
import { useSmabar } from "../store/bar";
import { OverlaySurface } from "./OverlaySurface";

type Listener = (event: { payload: unknown }) => void;

const { listeners, reportMeasureMock, pinMock } = vi.hoisted(() => ({
  listeners: new Map<string, Listener>(),
  reportMeasureMock: vi.fn(() => Promise.resolve()),
  pinMock: vi.fn(() => Promise.resolve()),
}));

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

let host: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  listeners.clear();
  reportMeasureMock.mockClear();
  pinMock.mockClear();
  useSmabar.setState(useSmabar.getInitialState(), true);
  useSmabar.setState({
    pluginUi: {
      "systeminfo/system/flyout": "SystemInfo content",
      "kitshow/kit/flyout": "UI Kit content",
    },
  });
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
  });
  const initialMeasures = reportMeasureMock.mock.calls.length;

  await act(async () => {
    useSmabar.getState().setPluginUi("crypto/crypto/tile", "unrelated");
    await Promise.resolve();
  });
  expect(reportMeasureMock).toHaveBeenCalledTimes(initialMeasures);

  await act(async () => {
    useSmabar
      .getState()
      .setPluginUi("systeminfo/system/flyout", "Updated system content");
    await Promise.resolve();
  });
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
    if (hover !== undefined)
      useSmabar.getState().setPluginUi("systeminfo/system/hover", hover);
    emit("surface-flyout", {
      generation: 1,
      tileId: "plugin:systeminfo:system",
      mode: "peek",
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
    useSmabar.setState({
      pluginUi: {
        ...(hover === undefined ? {} : { "systeminfo/system/hover": hover }),
        ...(full === undefined ? {} : { "systeminfo/system/flyout": full }),
      },
    });
    emit("surface-flyout", {
      generation: 7,
      tileId: "plugin:systeminfo:system",
      mode: "peek",
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
  useSmabar.getState().setPluginUi("systeminfo/system/hover", "Compact");
  emit("surface-flyout", {
    generation: 1,
    tileId: "plugin:systeminfo:system",
    mode: "peek",
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
  useSmabar
    .getState()
    .setPluginUi(
      "systeminfo/system/hover",
      '<button data-action="mute" title="Mute">Mute</button>',
    );
  emit("surface-flyout", {
    generation: 3,
    tileId: "plugin:systeminfo:system",
    mode: "peek",
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
  });
  const systemHost = host.querySelector("[data-plugin-id]");

  emit("surface-flyout", {
    generation: 2,
    tileId: "plugin:kitshow:kit",
    mode: "pinned",
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
  });
  // Concurrent staging in the core can deliver the older request late.
  emit("surface-flyout", {
    generation: 2,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
  });

  expect(host.querySelector("[data-plugin-id]")?.shadowRoot?.textContent).toBe(
    "UI Kit content",
  );
});

function emit(name: string, payload: unknown): void {
  const listener = listeners.get(name);
  if (listener === undefined) throw new Error(`missing ${name} listener`);
  act(() => {
    listener({ payload });
  });
}
