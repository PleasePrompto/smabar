// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../../store/bar";
import { ZoneDivider } from "./ZoneDivider";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

let container: HTMLDivElement;
let root: Root | null;
let row: HTMLDivElement;
let divider: HTMLDivElement;
let nextFrame: number;
let notifications: number;
let rectReads: number;
let rowLeft: number;
let rowWidth: number;
let unsubscribe: () => void;
const frames = new Map<number, FrameRequestCallback>();
const cancelFrame = vi.fn<(handle: number) => void>();

function installCapture(element: HTMLElement): void {
  const captured = new Set<number>();
  element.setPointerCapture = (pointerId) => captured.add(pointerId);
  element.hasPointerCapture = (pointerId) => captured.has(pointerId);
  element.releasePointerCapture = (pointerId) => captured.delete(pointerId);
}

function pointer(type: string, clientX: number, buttons: number): void {
  act(() => {
    divider.dispatchEvent(
      new PointerEvent(type, {
        pointerId: 9,
        isPrimary: true,
        button: 0,
        buttons,
        clientX,
        clientY: 20,
        bubbles: true,
      }),
    );
  });
}

function flushFrames(): void {
  const scheduled = [...frames.entries()];
  frames.clear();
  for (const [, callback] of scheduled) callback(0);
}

beforeEach(() => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
  cancelFrame.mockReset();
  cancelFrame.mockImplementation((handle) => {
    frames.delete(handle);
  });
  nextFrame = 1;
  rectReads = 0;
  rowLeft = 100;
  rowWidth = 1_000;
  frames.clear();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    const handle = nextFrame;
    nextFrame += 1;
    frames.set(handle, callback);
    return handle;
  });
  vi.stubGlobal("cancelAnimationFrame", cancelFrame);
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });

  const initial = useSmabar.getInitialState();
  useSmabar.setState(
    { ...initial, layout: { ...initial.layout, dividerRatio: 0.5 } },
    true,
  );
  notifications = 0;
  unsubscribe = useSmabar.subscribe((state, previous) => {
    if (state.layout.dividerRatio !== previous.layout.dividerRatio) {
      notifications += 1;
    }
  });

  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  act(() => {
    root?.render(
      <div data-row>
        <ZoneDivider />
      </div>,
    );
  });
  const foundRow = container.querySelector<HTMLDivElement>("[data-row]");
  const foundDivider =
    container.querySelector<HTMLDivElement>('[role="separator"]');
  if (foundRow === null || foundDivider === null) {
    throw new Error("divider fixture is incomplete");
  }
  row = foundRow;
  divider = foundDivider;
  row.getBoundingClientRect = () => {
    rectReads += 1;
    return new DOMRect(rowLeft, 0, rowWidth, 40);
  };
  installCapture(divider);
});

afterEach(() => {
  if (root !== null) act(() => root?.unmount());
  unsubscribe();
  container.remove();
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = false;
});

test("coalesces raw moves into one live store update per frame", () => {
  pointer("pointerdown", 600, 1);
  for (let index = 0; index < 20; index += 1) {
    pointer("pointermove", 420 + index * 20, 1);
  }

  expect(rectReads).toBe(1);
  expect(frames.size).toBe(1);
  expect(useSmabar.getState().layout.dividerRatio).toBe(0.5);
  expect(notifications).toBe(0);

  act(flushFrames);
  expect(rectReads).toBe(2);
  expect(useSmabar.getState().layout.dividerRatio).toBe(0.7);
  expect(notifications).toBe(1);
  vi.advanceTimersByTime(300);
  expect(invokeMock).toHaveBeenCalledOnce();
  expect(invokeMock).toHaveBeenLastCalledWith("update_config", {
    path: "layout.dividerRatio",
    value: 0.7,
  });
});

test("uses the current row geometry when a queued frame runs", () => {
  pointer("pointerdown", 600, 1);
  pointer("pointermove", 620, 1);
  rowLeft = 200;
  rowWidth = 600;

  act(flushFrames);

  expect(rectReads).toBe(2);
  expect(useSmabar.getState().layout.dividerRatio).toBe(0.7);
  expect(notifications).toBe(1);
});

test("pointer end flushes the final queued value exactly once", () => {
  pointer("pointerdown", 600, 1);
  pointer("pointermove", 750, 1);
  pointer("pointermove", 820, 1);
  expect(frames.size).toBe(1);

  pointer("pointerup", 820, 0);
  pointer("lostpointercapture", 820, 0);
  pointer("pointercancel", 820, 0);
  expect(frames.size).toBe(0);
  expect(cancelFrame).toHaveBeenCalledOnce();
  expect(useSmabar.getState().layout.dividerRatio).toBe(0.72);
  expect(notifications).toBe(1);

  vi.advanceTimersByTime(300);
  expect(invokeMock).toHaveBeenCalledOnce();
  expect(invokeMock).toHaveBeenLastCalledWith("update_config", {
    path: "layout.dividerRatio",
    value: 0.72,
  });
});

test("cleanup flushes an unpainted sample and its persistence", () => {
  pointer("pointerdown", 600, 1);
  pointer("pointermove", 900, 1);
  expect(frames.size).toBe(1);
  rowWidth = 0;

  act(() => root?.unmount());
  root = null;
  expect(frames.size).toBe(0);
  expect(useSmabar.getState().layout.dividerRatio).toBe(0.8);
  expect(notifications).toBe(1);
  expect(invokeMock).toHaveBeenCalledOnce();
  expect(invokeMock).toHaveBeenLastCalledWith("update_config", {
    path: "layout.dividerRatio",
    value: 0.8,
  });
});

test("auto width renders the divider as an inert separator", () => {
  const { layout } = useSmabar.getState();
  act(() => {
    useSmabar.setState({ layout: { ...layout, width: "auto" } });
  });
  const fixed = container.querySelector<HTMLDivElement>('[role="separator"]');
  if (fixed === null) throw new Error("divider missing at auto width");
  divider = fixed;
  expect(divider.hasAttribute("data-fixed")).toBe(true);
  expect(divider.className).not.toContain("cursor-col-resize");

  // A content-sized dock has no share to allocate: dragging changes nothing,
  // measures nothing and persists nothing.
  pointer("pointerdown", 600, 1);
  pointer("pointermove", 851, 1);
  flushFrames();
  pointer("pointerup", 851, 0);
  expect(useSmabar.getState().layout.dividerRatio).toBe(0.5);
  expect(rectReads).toBe(0);
  expect(invokeMock).not.toHaveBeenCalled();
});
