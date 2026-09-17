// @vitest-environment happy-dom
// @vitest-environment-options { "settings": { "navigation": { "disableChildFrameNavigation": true } } }
import { act } from "react";
import { expect, test, vi } from "vitest";

import { call } from "../ipc/call";
import { useSmabar } from "../store/bar";
import { recordMemoryUi } from "../ipc/memoryProbe";
import {
  installOverlayHarness,
  kitContent,
  systemContent,
  type Listener,
} from "./OverlaySurface.testkit";

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
const { emit, deliver } = installOverlayHarness(
  { listeners, reportMeasureMock, pinMock, invokeMock, takeQueue },
  (mounted) => {
    host = mounted;
  },
);

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

test("applies a sanitized width before the first measurement and updates without remounting", async () => {
  const widthSpy = vi
    .spyOn(HTMLElement.prototype, "offsetWidth", "get")
    .mockImplementation(function (this: HTMLElement) {
      const match = /^([\d.]+)(px|rem)$/.exec(this.style.width);
      return match === null
        ? 0
        : Number(match[1]) * (match[2] === "rem" ? 16 : 1);
    });
  try {
    emit("surface-flyout", {
      generation: 1,
      tileId: "plugin:systeminfo:system",
      mode: "pinned",
      content: {
        hover: null,
        flyout: '<section data-sb-flyout-width="wide">Sports</section>',
      },
    });
    expect(reportMeasureMock).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ width: 680 }),
    );
    const shadowHost = host.querySelector("[data-plugin-id]");
    const scroller = host.querySelector(".surface-scroll");
    if (scroller !== null) scroller.scrollTop = 42;
    for (const [html, expected] of [
      ['<section data-sb-flyout-width="720">Table</section>', 720],
      ['<section data-sb-flyout-width="720">Updated table</section>', 720],
      ["<section>Standard</section>", 340],
      ['<section data-sb-flyout-width="wide">Wide again</section>', 680],
      ["", 340],
    ] as const) {
      await deliver([
        {
          generation: 1,
          pluginId: "systeminfo",
          tileId: "system",
          target: "flyout",
          html,
        },
      ]);
      expect(reportMeasureMock).toHaveBeenLastCalledWith(
        expect.objectContaining({ width: expected }),
      );
      expect(host.querySelector("[data-plugin-id]")).toBe(shadowHost);
      expect(scroller?.scrollTop).toBe(42);
    }
    expect(reportMeasureMock).toHaveBeenCalledTimes(5);
    emit("surface-flyout", {
      generation: 2,
      tileId: "plugin:kitshow:kit",
      mode: "pinned",
      content: { hover: null, flyout: null },
    });
    expect(reportMeasureMock).toHaveBeenLastCalledWith(
      expect.objectContaining({ generation: 2, width: 340 }),
    );
  } finally {
    widthSpy.mockRestore();
  }
});

test("preview width follows the displayed content, preserves DOM on pin, and rejects stale updates", async () => {
  const full = '<section data-sb-flyout-width="wide">Full</section>';
  const preview = '<section data-sb-flyout-width="420">Preview</section>';
  emit("surface-flyout", {
    generation: 2,
    tileId: "plugin:systeminfo:system",
    mode: "peek",
    content: { hover: preview, flyout: full },
  });
  expect(host.firstElementChild?.getAttribute("style")).toContain(
    "width: 420px",
  );
  emit("surface-flyout", {
    generation: 2,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    preserveContent: false,
  });
  expect(host.firstElementChild?.getAttribute("style")).toContain(
    "width: 42.5rem",
  );
  await deliver(
    [
      {
        generation: 1,
        pluginId: "systeminfo",
        tileId: "system",
        target: "flyout",
        html: preview,
      },
    ],
    2,
  );
  expect(host.firstElementChild?.getAttribute("style")).toContain(
    "width: 42.5rem",
  );
  emit("surface-flyout", {
    generation: 3,
    tileId: "plugin:systeminfo:system",
    mode: "peek",
    content: { hover: null, flyout: full },
  });
  const content =
    host.querySelector("[data-plugin-id]")?.shadowRoot?.firstElementChild;
  emit("surface-flyout", {
    generation: 3,
    tileId: "plugin:systeminfo:system",
    mode: "pinned",
    preserveContent: true,
  });
  expect(
    host.querySelector("[data-plugin-id]")?.shadowRoot?.firstElementChild,
  ).toBe(content);
  expect(host.firstElementChild?.getAttribute("style")).toContain(
    "width: 42.5rem",
  );
});
