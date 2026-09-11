// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { pushNativePointerSample, pushPointerSample } from "../bar/useAutohide";
import {
  placeInline,
  TOOLTIP_DELAY_MS,
  TOOLTIP_WATCHDOG_MS,
  TooltipLayer,
} from "./Tooltip";

const { openTooltipMock, closeTooltipMock } = vi.hoisted(() => ({
  openTooltipMock: vi.fn(() => Promise.resolve()),
  closeTooltipMock: vi.fn(() => Promise.resolve()),
}));

vi.mock("../../ipc/overlay", () => ({
  openTooltipSurface: openTooltipMock,
  closeTooltipSurface: closeTooltipMock,
}));

let container: HTMLDivElement;
let reactRoot: Root;

beforeEach(() => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    value: 320,
  });
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: 200,
  });
  container = document.createElement("div");
  document.body.append(container);
  reactRoot = createRoot(container);
  act(() => {
    reactRoot.render(<TooltipLayer />);
  });
  openTooltipMock.mockClear();
  closeTooltipMock.mockClear();
});

afterEach(() => {
  act(() => {
    reactRoot.unmount();
  });
  container.remove();
  pushNativePointerSample(0, 0);
  vi.useRealTimers();
  vi.restoreAllMocks();
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = false;
});

test("a shadow-root tooltip is sent to the overlay after the dwell", () => {
  const host = document.createElement("div");
  document.body.append(host);
  const trigger = liveTrigger("Tooltip", {
    x: 280,
    y: 150,
    width: 20,
    height: 20,
  });
  host.attachShadow({ mode: "open" }).append(trigger);
  try {
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS - 1);
    });
    expect(openTooltipMock).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(openTooltipMock).toHaveBeenLastCalledWith("Tooltip", {
      left: 280,
      top: 150,
      width: 20,
      height: 20,
    });
  } finally {
    host.remove();
  }
});

test("a trigger reached by moving within a shadow root opens without a pointerover", () => {
  // Between two elements of one shadow tree the browser dispatches no
  // pointerover outside it; only the moves arrive at the document.
  const host = document.createElement("div");
  document.body.append(host);
  const trigger = liveTrigger("Reached from within", {
    x: 100,
    y: 100,
    width: 40,
    height: 20,
  });
  host.attachShadow({ mode: "open" }).append(trigger);
  try {
    act(() => {
      trigger.dispatchEvent(
        new PointerEvent("pointermove", {
          bubbles: true,
          composed: true,
          clientX: 110,
          clientY: 110,
        }),
      );
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    expect(openTooltipMock).toHaveBeenLastCalledWith("Reached from within", {
      left: 100,
      top: 100,
      width: 40,
      height: 20,
    });
  } finally {
    host.remove();
  }
});

test("an active tile hover flyout suppresses nested tooltips", () => {
  const tile = document.createElement("div");
  tile.dataset.hoverFlyout = "";
  const host = document.createElement("div");
  const trigger = liveTrigger("Redundant tooltip");
  host.attachShadow({ mode: "open" }).append(trigger);
  tile.append(host);
  document.body.append(tile);
  try {
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    expect(openTooltipMock).not.toHaveBeenCalled();
  } finally {
    tile.remove();
  }
});

test("native leave dismisses pointer tooltips but preserves keyboard focus", () => {
  const trigger = liveTrigger("Native leave");
  document.body.append(trigger);
  try {
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    expect(openTooltipMock).toHaveBeenCalledTimes(1);
    act(() => {
      pushNativePointerSample(Number.NaN, Number.NaN);
    });
    expect(closeTooltipMock).toHaveBeenCalledTimes(1);
    act(() => {
      trigger.focus();
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
      pushNativePointerSample(Number.NaN, Number.NaN);
    });
    expect(openTooltipMock).toHaveBeenCalledTimes(2);
    expect(closeTooltipMock).toHaveBeenCalledTimes(1);
  } finally {
    trigger.remove();
  }
});

test("a native pointer sample outside the trigger dismisses its tooltip", () => {
  const trigger = liveTrigger("Outside sample");
  document.body.append(trigger);
  try {
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    expect(openTooltipMock).toHaveBeenCalledTimes(1);
    act(() => {
      pushPointerSample(200, 180);
    });
    expect(closeTooltipMock).toHaveBeenCalledTimes(1);
  } finally {
    trigger.remove();
  }
});

test("keyboard focus opens the tooltip and focusout or Escape dismisses it", () => {
  const trigger = liveTrigger("Keyboard tooltip");
  const outside = document.createElement("button");
  document.body.append(trigger, outside);
  try {
    act(() => {
      trigger.focus();
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    expect(openTooltipMock).toHaveBeenLastCalledWith(
      "Keyboard tooltip",
      expect.any(Object),
    );
    act(() => {
      outside.focus();
    });
    expect(closeTooltipMock).toHaveBeenCalledTimes(1);
    act(() => {
      trigger.focus();
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    expect(openTooltipMock).toHaveBeenCalledTimes(2);
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(closeTooltipMock).toHaveBeenCalledTimes(2);
  } finally {
    trigger.remove();
    outside.remove();
  }
});

test("losing application focus dismisses a focused tooltip", () => {
  const trigger = liveTrigger("Focused tooltip");
  document.body.append(trigger);
  try {
    act(() => {
      trigger.focus();
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    expect(openTooltipMock).toHaveBeenCalledTimes(1);
    act(() => {
      window.dispatchEvent(new Event("blur"));
    });
    expect(closeTooltipMock).toHaveBeenCalledTimes(1);
  } finally {
    trigger.remove();
  }
});

test("a live plugin render updates an open tooltip", () => {
  const host = document.createElement("div");
  document.body.append(host);
  const shadow = host.attachShadow({ mode: "open" });
  const trigger = liveTrigger("CPU 5%");
  shadow.append(trigger);
  try {
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    const replacement = liveTrigger("CPU 25%");
    act(() => {
      trigger.dispatchEvent(
        new PointerEvent("pointerout", { bubbles: true, composed: true }),
      );
      trigger.replaceWith(replacement);
      hover(replacement);
    });
    expect(openTooltipMock).toHaveBeenLastCalledWith(
      "CPU 25%",
      expect.any(Object),
    );
  } finally {
    host.remove();
  }
});

test("an anchor that dies during the dwell never opens a tooltip", () => {
  const trigger = liveTrigger("Doomed");
  document.body.append(trigger);
  act(() => {
    hover(trigger);
    trigger.remove();
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
  });
  expect(openTooltipMock).not.toHaveBeenCalled();
});

test("the watchdog closes an orphaned tooltip", () => {
  const trigger = liveTrigger("Orphaned");
  document.body.append(trigger);
  act(() => {
    hover(trigger);
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
  });
  act(() => {
    trigger.remove();
    vi.advanceTimersByTime(TOOLTIP_WATCHDOG_MS);
  });
  expect(closeTooltipMock).toHaveBeenCalledTimes(1);
});

test("switching to a collapsed anchor clears the tooltip", () => {
  const live = liveTrigger("Live");
  const collapsed = liveTrigger("Collapsed", {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
  });
  document.body.append(live, collapsed);
  try {
    act(() => {
      hover(live);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    act(() => {
      hover(collapsed);
    });
    expect(closeTooltipMock).toHaveBeenCalledTimes(1);
  } finally {
    live.remove();
    collapsed.remove();
  }
});

test("the watchdog closes a tooltip when autohide retracts", () => {
  let rect = DOMRect.fromRect({ x: 20, y: 20, width: 40, height: 20 });
  const trigger = liveTrigger("Retracted", () => rect);
  document.body.append(trigger);
  try {
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    act(() => {
      rect = DOMRect.fromRect({ x: 20, y: 192, width: 40, height: 20 });
      vi.advanceTimersByTime(TOOLTIP_WATCHDOG_MS);
    });
    expect(closeTooltipMock).toHaveBeenCalledTimes(1);
  } finally {
    trigger.remove();
  }
});

test("a failed dwell retries after the anchor becomes renderable", () => {
  let rect = DOMRect.fromRect({ x: 20, y: 210, width: 40, height: 20 });
  const trigger = liveTrigger("Retry", () => rect);
  document.body.append(trigger);
  try {
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    expect(openTooltipMock).not.toHaveBeenCalled();
    rect = DOMRect.fromRect({ x: 20, y: 20, width: 40, height: 20 });
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    expect(openTooltipMock).toHaveBeenCalledTimes(1);
  } finally {
    trigger.remove();
  }
});

test("the watchdog follows a moving anchor", () => {
  let rect = DOMRect.fromRect({ x: 20, y: 20, width: 40, height: 20 });
  const trigger = liveTrigger("Moving", () => rect);
  document.body.append(trigger);
  try {
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    rect = DOMRect.fromRect({ x: 240, y: 20, width: 40, height: 20 });
    act(() => {
      vi.advanceTimersByTime(TOOLTIP_WATCHDOG_MS);
    });
    expect(openTooltipMock).toHaveBeenLastCalledWith("Moving", {
      left: 240,
      top: 20,
      width: 40,
      height: 20,
    });
  } finally {
    trigger.remove();
  }
});

test("inside the overlay window a tooltip renders inline instead of opening a surface", () => {
  act(() => {
    reactRoot.render(<TooltipLayer inline />);
  });
  const trigger = liveTrigger("Inline", {
    x: 100,
    y: 100,
    width: 40,
    height: 20,
  });
  document.body.append(trigger);
  try {
    act(() => {
      hover(trigger);
      vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    });
    const tooltip = container.querySelector(".overlay-tooltip");
    expect(tooltip?.textContent).toBe("Inline");
    expect(tooltip?.getAttribute("aria-hidden")).toBe("true");
    expect(openTooltipMock).not.toHaveBeenCalled();
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(container.querySelector(".overlay-tooltip")).toBeNull();
  } finally {
    trigger.remove();
  }
});

test("an inline tooltip sits above its trigger when there is room and below otherwise", () => {
  const size = { width: 100, height: 30 };
  const viewport = { width: 320, height: 200 };
  expect(
    placeInline({ left: 100, top: 120, width: 40, height: 20 }, size, viewport),
  ).toEqual({ side: "top", x: 70, y: 82 });
  // A header button: no room above, and the box is pulled back inside the
  // right edge.
  expect(
    placeInline({ left: 300, top: 10, width: 20, height: 20 }, size, viewport),
  ).toEqual({ side: "bottom", x: 212, y: 38 });
});

type RectSource = DOMRect | (() => DOMRect);

function liveTrigger(
  text: string,
  source:
    RectSource | { x: number; y: number; width: number; height: number } = {
    x: 20,
    y: 20,
    width: 40,
    height: 20,
  },
) {
  const trigger = document.createElement("button");
  trigger.dataset.sbTooltip = text;
  const initial =
    typeof source === "function" ? null : DOMRect.fromRect(source);
  Object.defineProperty(trigger, "getBoundingClientRect", {
    value: () => {
      if (!trigger.isConnected) return new DOMRect(0, 0, 0, 0);
      return typeof source === "function" ? source() : initial;
    },
  });
  return trigger;
}

function hover(element: HTMLElement) {
  element.dispatchEvent(
    new PointerEvent("pointerover", {
      bubbles: true,
      composed: true,
      clientX: 25,
      clientY: 25,
    }),
  );
}
