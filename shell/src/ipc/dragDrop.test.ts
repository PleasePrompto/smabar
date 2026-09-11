// @vitest-environment happy-dom
import { PhysicalPosition } from "@tauri-apps/api/dpi";
import type { EventCallback } from "@tauri-apps/api/event";
import type { DragDropEvent } from "@tauri-apps/api/webview";
import { afterAll, afterEach, beforeEach, expect, test, vi } from "vitest";

import { subscribePointerSamples } from "../components/bar/useAutohide";
import { useSmabar } from "../store/bar";
import { initDragDrop } from "./dragDrop";

type DropListener = EventCallback<DragDropEvent>;

const {
  invokeMock,
  registerMock,
  reportErrorMock,
  showNoticeMock,
  subscribePointerMock,
} = vi.hoisted(() => ({
  subscribePointerMock: vi.fn(),
  invokeMock: vi.fn(),
  registerMock: vi.fn(),
  reportErrorMock: vi.fn(),
  showNoticeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ onDragDropEvent: registerMock }),
}));
vi.mock("./log", () => ({ reportError: reportErrorMock }));
vi.mock("./surface", () => ({ showNotice: showNoticeMock }));
vi.mock("../components/bar/useAutohide", () => ({
  pushPointerSample: vi.fn(),
  subscribePointerSamples: subscribePointerMock,
}));

let listener: DropListener | undefined;
const documentEvents = vi.spyOn(document, "addEventListener");

beforeEach(() => {
  documentEvents.mockClear();
  listener = undefined;
  subscribePointerMock.mockReset();
  invokeMock.mockReset();
  registerMock.mockReset();
  reportErrorMock.mockReset();
  showNoticeMock.mockReset();
  showNoticeMock.mockResolvedValue(undefined);
  registerMock.mockImplementation((next: DropListener) => {
    listener = next;
    return Promise.resolve(() => undefined);
  });
  useSmabar.setState(useSmabar.getInitialState(), true);
  const zone = document.createElement("div");
  zone.dataset.shortcutZone = "";
  zone.getBoundingClientRect = () => new DOMRect(0, 0, 200, 100);
  document.body.replaceChildren(zone);
});

afterEach(() => {
  for (const [type, callback, options] of documentEvents.mock.calls) {
    document.removeEventListener(type, callback, options);
  }
});

afterAll(() => {
  documentEvents.mockRestore();
});

const position = new PhysicalPosition(50, 50);

function emitDrag(payload: DragDropEvent): void {
  expect(listener).toBeDefined();
  listener?.({ event: "tauri://drag", id: 0, payload });
}

function enterDrag(): void {
  emitDrag({ type: "enter", paths: ["/tmp/folder"], position });
  expect(useSmabar.getState().dropActive).toBe(true);
  expect(useSmabar.getState().fileDrag).toBe(true);
}

function expectDragEnded(): void {
  expect(useSmabar.getState().dropActive).toBe(false);
  expect(useSmabar.getState().fileDrag).toBe(false);
}

test("pointer release clears a late native Enter after a rejected duplicate", async () => {
  invokeMock.mockRejectedValue(new Error("already pinned"));
  await initDragDrop("bar");
  enterDrag();
  emitDrag({ type: "drop", paths: ["/tmp/folder"], position });
  await vi.waitFor(() => {
    expect(showNoticeMock).toHaveBeenCalled();
  });
  expectDragEnded();

  // Recorded on Linux: a late Enter without a following Drop or Leave.
  emitDrag({ type: "enter", paths: ["/tmp/folder"], position });
  document.dispatchEvent(new PointerEvent("pointermove", { buttons: 0 }));
  expectDragEnded();
  enterDrag();
});

test.each(["pointermove", "pointerup"])(
  "%s with no held button ends a file drag even without native Drop or Leave",
  async (type) => {
    await initDragDrop("bar");
    enterDrag();
    document.dispatchEvent(new PointerEvent("pointermove", { buttons: 1 }));
    expect(useSmabar.getState().dropActive).toBe(true);
    document.dispatchEvent(new PointerEvent(type, { buttons: 0 }));
    expectDragEnded();
    enterDrag();
  },
);

test("native pointer exit clears a drag without a native Leave", async () => {
  await initDragDrop("bar");
  enterDrag();
  const onPointerSample = vi.mocked(subscribePointerSamples).mock.calls[0]?.[0];
  expect(onPointerSample).toBeDefined();
  onPointerSample?.(Number.NaN, Number.NaN);
  expectDragEnded();
});

test("native Over still reveals the target when Enter has no file paths yet", async () => {
  await initDragDrop("bar");
  emitDrag({ type: "enter", paths: [], position });
  emitDrag({ type: "over", position });
  expect(useSmabar.getState().dropActive).toBe(true);
  emitDrag({ type: "leave" });
  expectDragEnded();
});

test("Escape ends a file drag without a native Leave", async () => {
  await initDragDrop("bar");
  enterDrag();
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  expectDragEnded();
});

test("a multi-file drop pins in order and continues after one failure", async () => {
  let releaseFirst: (() => void) | undefined;
  invokeMock
    .mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          releaseFirst = resolve;
        }),
    )
    .mockRejectedValueOnce(new Error("duplicate"))
    .mockResolvedValueOnce(undefined);

  await initDragDrop("bar");
  enterDrag();
  emitDrag({
    type: "drop",
    paths: ["/tmp/one.txt", "/tmp/two.pdf", "/tmp/three.png"],
    position,
  });
  expectDragEnded();

  expect(invokeMock).toHaveBeenCalledTimes(1);
  releaseFirst?.();
  await vi.waitFor(() => {
    expect(invokeMock).toHaveBeenCalledTimes(3);
    expect(showNoticeMock).toHaveBeenCalledWith("shortcuts.dropFailed");
  });
  expect(invokeMock.mock.calls).toEqual([
    ["pin_shortcut", { path: "/tmp/one.txt" }],
    ["pin_shortcut", { path: "/tmp/two.pdf" }],
    ["pin_shortcut", { path: "/tmp/three.png" }],
  ]);
  expect(reportErrorMock).toHaveBeenCalledTimes(1);
});
