// @vitest-environment happy-dom
// @vitest-environment-options { "settings": { "navigation": { "disableChildFrameNavigation": true } } }
import { act, useLayoutEffect, useRef } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, test, vi } from "vitest";

import { actionElementInPath, ShadowHost } from "./PluginContent";
import { setEmbedRoot } from "./embeds";

const { callMock } = vi.hoisted(() => ({
  callMock: vi.fn(() => Promise.resolve()),
}));
vi.mock("../ipc/call", () => ({ call: callMock }));

let mounted: HTMLElement | null = null;

afterEach(() => {
  vi.useRealTimers();
  callMock.mockClear();
  setEmbedRoot("");
  mounted?.remove();
  mounted = null;
});

function actionFrom(markup: string): {
  action: HTMLElement | null;
  event: MouseEvent;
} {
  const host = document.createElement("div");
  document.body.append(host);
  mounted = host;
  const root = host.attachShadow({ mode: "open" });
  root.innerHTML = markup;
  const button = root.querySelector("button");
  if (button === null) throw new Error("button missing");
  let action: HTMLElement | null = null;
  host.addEventListener("click", (event) => {
    action = actionElementInPath(event.composedPath(), host);
  });
  const event = new MouseEvent("click", {
    bubbles: true,
    cancelable: true,
    composed: true,
  });
  button.dispatchEvent(event);
  return { action, event };
}

test("plugin markup exists before a parent layout measurement", () => {
  const reactHost = document.createElement("div");
  document.body.append(reactHost);
  const reactRoot = createRoot(reactHost);
  const observations: boolean[] = [];

  function Harness() {
    const container = useRef<HTMLDivElement>(null);
    useLayoutEffect(() => {
      observations.push(
        container.current?.firstElementChild?.shadowRoot?.textContent ===
          "ready",
      );
    }, []);
    return (
      <div ref={container}>
        <ShadowHost
          pluginId="systeminfo"
          tileId="system"
          html="<span>ready</span>"
        />
      </div>
    );
  }

  try {
    act(() => {
      reactRoot.render(<Harness />);
    });
    expect(observations).toEqual([true]);
  } finally {
    act(() => {
      reactRoot.unmount();
    });
    reactHost.remove();
  }
});

test("data-action wins when a native command handles the same click", () => {
  const { action, event } = actionFrom(
    '<button data-action="clear" commandfor="confirm" command="close">Clear</button>',
  );
  expect(action?.dataset.action).toBe("clear");
  expect(event.defaultPrevented).toBe(false);
});

test("data-action wins when a kit menu item handles the same click", () => {
  const { action, event } = actionFrom(
    '<button role="menuitem" data-action="refresh">Refresh</button>',
  );
  expect(action?.dataset.action).toBe("refresh");
  expect(event.defaultPrevented).toBe(false);
});

test("remote players exist only in pinned flyouts and survive unchanged renders", () => {
  setEmbedRoot("http://127.0.0.1:4242/embed");
  const reactHost = document.createElement("div");
  document.body.append(reactHost);
  const reactRoot = createRoot(reactHost);
  const markup = (id: string) =>
    `<iframe src="https://www.youtube-nocookie.com/embed/${id}" title="Video"></iframe>`;

  try {
    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="latest-video"
          tileId="video"
          target="tile"
          html={markup("one")}
        />,
      );
    });
    const shadow = reactHost.firstElementChild?.shadowRoot;
    expect(shadow?.querySelector("iframe")).toBeNull();

    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="latest-video"
          tileId="video"
          target="flyout"
          allowEmbeds
          html={markup("one")}
        />,
      );
    });
    const playing = shadow?.querySelector("iframe");
    expect(playing).not.toBeNull();
    expect(playing?.getAttribute("title")).toBe("Video");
    expect(playing?.hasAttribute("data-sb-tooltip")).toBe(false);

    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="latest-video"
          tileId="video"
          target="flyout"
          allowEmbeds
          html={markup("one")}
        />,
      );
    });
    expect(shadow?.querySelector("iframe")).toBe(playing);

    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="latest-video"
          tileId="video"
          target="flyout"
          allowEmbeds
          html={markup("two")}
        />,
      );
    });
    expect(shadow?.querySelector("iframe")).not.toBe(playing);

    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="latest-video"
          tileId="video"
          target="popup"
          html={markup("two")}
        />,
      );
    });
    expect(shadow?.querySelector("iframe")).toBeNull();
  } finally {
    act(() => {
      reactRoot.unmount();
    });
    reactHost.remove();
  }
});

test("concurrent popup instances keep independent value memory", () => {
  const reactHost = document.createElement("div");
  document.body.append(reactHost);
  const reactRoot = createRoot(reactHost);
  const popup = (scope: string, value: number) => (
    <ShadowHost
      key={scope}
      pluginId="alerts"
      tileId="main"
      target="popup"
      memoryScope={scope}
      html={`<span data-sb-tween data-sb-key="value">${String(value)}</span>`}
    />
  );

  try {
    act(() => {
      reactRoot.render(
        <>
          {popup("first", 10)}
          {popup("second", 80)}
        </>,
      );
    });
    const values = [...reactHost.querySelectorAll("[data-plugin-id]")].map(
      (host) => host.shadowRoot?.textContent,
    );
    expect(values).toEqual(["10", "80"]);

    act(() => {
      reactRoot.render(popup("second", 100));
    });
    expect(
      reactHost.querySelector("[data-plugin-id]")?.shadowRoot?.textContent,
    ).toBe("80");
  } finally {
    act(() => {
      reactRoot.unmount();
    });
    reactHost.remove();
  }
});

test("changing flyout content sources starts with fresh value memory", () => {
  const reactHost = document.createElement("div");
  document.body.append(reactHost);
  const reactRoot = createRoot(reactHost);

  try {
    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="weather"
          tileId="main"
          target="flyout"
          memoryScope="hover"
          html='<span data-sb-tween data-sb-key="value">10</span>'
        />,
      );
    });
    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="weather"
          tileId="main"
          target="flyout"
          memoryScope="flyout"
          html='<span data-sb-tween data-sb-key="value">90</span>'
        />,
      );
    });
    expect(reactHost.firstElementChild?.shadowRoot?.textContent).toBe("90");
  } finally {
    act(() => {
      reactRoot.unmount();
    });
    reactHost.remove();
  }
});

test("provider renders do not replace a hovered or active range", async () => {
  vi.useFakeTimers();
  const reactHost = document.createElement("div");
  document.body.append(reactHost);
  const reactRoot = createRoot(reactHost);
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  const markup = (cpu: number, volume = 10) =>
    `<span data-cpu>CPU ${String(cpu)}%</span>` +
    `<div data-sb-range data-sb-range-unit="%">` +
    `<input type="range" value="${String(volume)}" data-action="setVolume" data-field="volume">` +
    `<output class="sb-range-wrap__bubble"></output></div>`;

  try {
    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="systeminfo"
          tileId="system"
          target="flyout"
          html={markup(5)}
        />,
      );
    });
    const shadow = reactHost.firstElementChild?.shadowRoot;
    const range = shadow?.querySelector<HTMLInputElement>(
      'input[type="range"]',
    );
    if (range === null || range === undefined) throw new Error("range missing");

    act(() => {
      range.dispatchEvent(
        new PointerEvent("pointerenter", {
          composed: true,
          pointerId: 1,
        }),
      );
      reactRoot.render(
        <ShadowHost
          pluginId="systeminfo"
          tileId="system"
          target="flyout"
          html={markup(25)}
        />,
      );
    });

    expect(shadow?.querySelector('input[type="range"]')).toBe(range);
    expect(shadow?.querySelector("[data-cpu]")?.textContent).toBe("CPU 25%");

    act(() => {
      range.dispatchEvent(
        new PointerEvent("pointerdown", {
          bubbles: true,
          composed: true,
          pointerId: 1,
        }),
      );
      reactRoot.render(
        <ShadowHost
          pluginId="systeminfo"
          tileId="system"
          target="flyout"
          html={markup(25)}
        />,
      );
    });

    expect(shadow?.querySelector('input[type="range"]')).toBe(range);
    expect(shadow?.querySelector("[data-cpu]")?.textContent).toBe("CPU 25%");

    range.value = "80";
    await act(async () => {
      window.dispatchEvent(new PointerEvent("pointerup", { pointerId: 1 }));
      range.dispatchEvent(
        new Event("change", { bubbles: true, composed: true }),
      );
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(shadow?.querySelector('input[type="range"]')).toBe(range);
    expect(shadow?.querySelector("[data-cpu]")?.textContent).toBe("CPU 25%");
    expect(
      shadow?.querySelector<HTMLInputElement>('input[type="range"]')?.value,
    ).toBe("80");
    expect(callMock).toHaveBeenCalledTimes(1);
    expect(callMock).toHaveBeenCalledWith("plugin_action", {
      pluginId: "systeminfo",
      tileId: "system",
      action: "setVolume",
      value: "80",
    });

    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="systeminfo"
          tileId="system"
          target="flyout"
          html={markup(30, 80)}
        />,
      );
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    expect(shadow?.querySelector('input[type="range"]')).toBe(range);
    expect(shadow?.querySelector("[data-cpu]")?.textContent).toBe("CPU 30%");
    expect(range.value).toBe("80");

    await act(async () => {
      range.dispatchEvent(
        new PointerEvent("pointerleave", {
          composed: true,
          pointerId: 1,
        }),
      );
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(shadow?.querySelector("[data-cpu]")?.textContent).toBe("CPU 30%");
    expect(
      shadow?.querySelector<HTMLInputElement>('input[type="range"]')?.value,
    ).toBe("80");

    const keyboardRange = shadow?.querySelector<HTMLInputElement>(
      'input[type="range"]',
    );
    if (keyboardRange === null || keyboardRange === undefined) {
      throw new Error("keyboard range missing");
    }
    keyboardRange.focus();
    act(() => {
      keyboardRange.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "ArrowRight",
          bubbles: true,
          composed: true,
        }),
      );
      keyboardRange.value = "81";
      keyboardRange.dispatchEvent(
        new Event("change", { bubbles: true, composed: true }),
      );
      reactRoot.render(
        <ShadowHost
          pluginId="systeminfo"
          tileId="system"
          target="flyout"
          html={markup(45, 80)}
        />,
      );
    });
    expect(shadow?.querySelector('input[type="range"]')).toBe(keyboardRange);
    expect(shadow?.querySelector("[data-cpu]")?.textContent).toBe("CPU 30%");

    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keyup", { key: "ArrowRight" }));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(shadow?.querySelector("[data-cpu]")?.textContent).toBe("CPU 45%");
    const releasedRange = shadow?.querySelector<HTMLInputElement>(
      'input[type="range"]',
    );
    expect(releasedRange?.value).toBe("81");
    expect(shadow?.activeElement).toBe(releasedRange);
    expect(callMock).toHaveBeenCalledTimes(2);
    act(() => {
      reactRoot.render(
        <ShadowHost
          pluginId="systeminfo"
          tileId="system"
          target="flyout"
          html={markup(45, 81)}
        />,
      );
    });

    const timeoutRange = shadow?.querySelector<HTMLInputElement>(
      'input[type="range"]',
    );
    const bubble = shadow?.querySelector(".sb-range-wrap__bubble");
    if (
      timeoutRange === null ||
      timeoutRange === undefined ||
      bubble === null ||
      bubble === undefined
    ) {
      throw new Error("timeout range missing");
    }
    timeoutRange.value = "90";
    bubble.textContent = "90%";
    act(() => {
      timeoutRange.dispatchEvent(
        new Event("change", { bubbles: true, composed: true }),
      );
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    expect(timeoutRange.value).toBe("81");
    expect(bubble.textContent).toBe("81%");
    expect(callMock).toHaveBeenCalledTimes(3);
  } finally {
    act(() => {
      reactRoot.unmount();
    });
    reactHost.remove();
    (
      globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = false;
  }
});
