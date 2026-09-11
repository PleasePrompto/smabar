// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../store/bar";
import { pushNativePointerSample } from "./bar/useAutohide";
import { useFlyoutTrigger } from "./useFlyoutTrigger";
import { publishOverlayPointer } from "../ipc/overlay";

let container: HTMLDivElement;
let root: Root;
const actEnvironment = globalThis as typeof globalThis & {
  IS_REACT_ACT_ENVIRONMENT?: boolean;
};
let previousActEnvironment: boolean | undefined;

function Harness() {
  const { ref, peekEnter, peekLeave } = useFlyoutTrigger("weather");
  return (
    <>
      <button
        ref={ref}
        data-trigger
        onMouseEnter={peekEnter}
        onMouseLeave={peekLeave}
      >
        Weather
      </button>
    </>
  );
}

beforeEach(() => {
  previousActEnvironment = actEnvironment.IS_REACT_ACT_ENVIRONMENT;
  actEnvironment.IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
  useSmabar.setState(useSmabar.getInitialState(), true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  act(() => {
    root.render(<Harness />);
  });
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  pushNativePointerSample(0, 0);
  vi.useRealTimers();
  actEnvironment.IS_REACT_ACT_ENVIRONMENT = previousActEnvironment;
});

test("native Wayland leave closes a hover peek without a DOM mouseleave", () => {
  const trigger = container.querySelector("[data-trigger]");
  if (trigger === null) throw new Error("flyout trigger missing");
  vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue(
    DOMRect.fromRect({ x: 20, y: 80, width: 40, height: 20 }),
  );

  act(() => {
    trigger.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(400);
  });
  expect(useSmabar.getState().flyoutMode).toBe("peek");

  act(() => {
    pushNativePointerSample(Number.NaN, Number.NaN);
    vi.advanceTimersByTime(159);
  });
  expect(useSmabar.getState().flyoutMode).toBe("peek");

  act(() => {
    pushNativePointerSample(30, 90);
    vi.advanceTimersByTime(1);
  });
  expect(useSmabar.getState().flyoutMode).toBe("peek");

  act(() => {
    pushNativePointerSample(Number.NaN, Number.NaN);
    vi.advanceTimersByTime(160);
  });
  expect(useSmabar.getState().flyoutMode).toBeNull();
});

test("entering the overlay cancels the cross-window leave grace", () => {
  const trigger = container.querySelector("[data-trigger]");
  if (trigger === null) {
    throw new Error("flyout harness missing");
  }
  vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue(
    DOMRect.fromRect({ x: 20, y: 80, width: 40, height: 20 }),
  );

  act(() => {
    trigger.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(400);
    pushNativePointerSample(Number.NaN, Number.NaN);
    publishOverlayPointer(true);
    vi.advanceTimersByTime(160);
  });

  expect(useSmabar.getState().flyoutMode).toBe("peek");

  act(() => {
    publishOverlayPointer(false);
    vi.advanceTimersByTime(160);
  });
  expect(useSmabar.getState().flyoutMode).toBeNull();
});

test("a native leave reported after the overlay enter keeps the peek open", () => {
  const trigger = container.querySelector("[data-trigger]");
  if (trigger === null) {
    throw new Error("flyout harness missing");
  }
  vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue(
    DOMRect.fromRect({ x: 20, y: 80, width: 40, height: 20 }),
  );

  // The overlay sits over the bar window's lower edge: the pointer enters
  // it first, and the bar's watchdog reports the leave only afterwards.
  act(() => {
    trigger.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(400);
    publishOverlayPointer(true);
    pushNativePointerSample(Number.NaN, Number.NaN);
    vi.advanceTimersByTime(160);
  });
  expect(useSmabar.getState().flyoutMode).toBe("peek");

  act(() => {
    publishOverlayPointer(false);
    vi.advanceTimersByTime(160);
  });
  expect(useSmabar.getState().flyoutMode).toBeNull();
});

test("the first hover waits for the native watchdog to confirm entry", () => {
  const trigger = container.querySelector("[data-trigger]");
  if (trigger === null) throw new Error("flyout trigger missing");
  vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue(
    DOMRect.fromRect({ x: 20, y: 80, width: 40, height: 20 }),
  );
  act(() => {
    pushNativePointerSample(Number.NaN, Number.NaN);
    trigger.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(150);
    pushNativePointerSample(30, 90);
    vi.advanceTimersByTime(400);
  });
  expect(useSmabar.getState().flyoutMode).toBe("peek");
});

test("native motion opens and leaves a tile without DOM events or resetting its delay", () => {
  const trigger = container.querySelector("[data-trigger]");
  if (trigger === null) throw new Error("flyout trigger missing");
  vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue(
    DOMRect.fromRect({ x: 20, y: 80, width: 40, height: 20 }),
  );
  act(() => {
    pushNativePointerSample(30, 90);
    vi.advanceTimersByTime(200);
    pushNativePointerSample(31, 90);
    vi.advanceTimersByTime(200);
  });
  expect(useSmabar.getState().flyoutMode).toBe("peek");
  act(() => {
    pushNativePointerSample(100, 90);
    vi.advanceTimersByTime(160);
  });
  expect(useSmabar.getState().flyoutMode).toBeNull();
});

test("a stale DOM hover cannot open a peek while the native pointer is outside", () => {
  const trigger = container.querySelector("[data-trigger]");
  if (trigger === null) throw new Error("flyout trigger missing");
  vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue(
    DOMRect.fromRect({ x: 20, y: 80, width: 40, height: 20 }),
  );
  act(() => {
    pushNativePointerSample(Number.NaN, Number.NaN);
    trigger.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    vi.advanceTimersByTime(400);
    pushNativePointerSample(100, 90);
    vi.advanceTimersByTime(400);
  });
  expect(useSmabar.getState().flyoutMode).toBeNull();
  act(() => {
    trigger.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
    pushNativePointerSample(Number.NaN, Number.NaN);
    vi.advanceTimersByTime(400);
  });
  expect(useSmabar.getState().flyoutMode).toBeNull();
});
