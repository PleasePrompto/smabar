// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";

interface EventEnvelope<T> {
  payload: T;
}

type Listener = (event: EventEnvelope<unknown>) => void;
interface ListenerTarget {
  kind: "WebviewWindow";
  label: string;
}

interface ReplyEnvelope {
  reply: {
    id: number;
    error?: string;
    targets: string[];
  };
}

const listeners = new Map<string, Listener>();
const listenerTargets = new Map<string, ListenerTarget | undefined>();
const replies: ReplyEnvelope[] = [];

vi.mock("@tauri-apps/api/event", () => ({
  listen: (
    name: string,
    listener: Listener,
    options?: { target?: ListenerTarget },
  ) => {
    listeners.set(name, listener);
    listenerTargets.set(name, options?.target);
    return Promise.resolve(() => undefined);
  },
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, payload: ReplyEnvelope) => {
    if (command === "bar_reply") replies.push(payload);
    return Promise.resolve();
  },
}));

import { initCapture } from "./capture";
import { useSmabar } from "../store/bar";

beforeEach(() => {
  document.body.innerHTML = '<button data-tile-id="plugin:demo:main"></button>';
  listeners.clear();
  listenerTargets.clear();
  replies.length = 0;
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    value: 320,
  });
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: 200,
  });
  useSmabar.setState(useSmabar.getInitialState(), true);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function immediateFrames(): void {
  let frame = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frame += 1;
    queueMicrotask(() => {
      callback(frame);
    });
    return frame;
  });
}

test.each([
  ["overlay", "flyout"],
  ["bar", "overlay"],
] as const)("captures %s's %s target", async (surface, target) => {
  document.body.innerHTML = `<div data-capture="${target}">content</div>`;
  immediateFrames();

  await initCapture(surface);
  expect(listenerTargets.get("bar-capture")).toEqual({
    kind: "WebviewWindow",
    label: surface,
  });
  const listener = listeners.get("bar-capture");
  if (listener === undefined) throw new Error("bar-capture listener missing");
  listener({
    payload: {
      id: 7,
      target,
    },
  });

  await vi.waitFor(() => {
    expect(replies).toHaveLength(1);
  });
  expect(replies[0]).toMatchObject({
    reply: {
      id: 7,
    },
  });
  expect(replies[0]?.reply.targets).toContain(target);
  expect(replies[0]?.reply.error).toBeUndefined();
});

test.each(["split", "rows", "solo"] as const)(
  "opening overlay in %s preserves the layout and requires solo",
  async (variant) => {
    const state = useSmabar.getState();
    state.setLayout({ ...state.layout, variant });
    immediateFrames();
    await initCapture("bar");
    const listener = listeners.get("bar-ui-command");
    if (listener === undefined)
      throw new Error("bar-ui-command listener missing");
    listener({ payload: { id: 11, action: "open_overlay", tileId: null } });
    await vi.waitFor(() => {
      expect(replies).toHaveLength(1);
    });
    expect(replies[0]?.reply.error).toBe(
      variant === "solo"
        ? undefined
        : "the secondary row requires the solo layout; select solo before opening overlay",
    );
    expect(useSmabar.getState().layout.variant).toBe(variant);
    expect(useSmabar.getState().overlayOpen).toBe(variant === "solo");

    listener({ payload: { id: 12, action: "close_overlay", tileId: null } });
    await vi.waitFor(() => {
      expect(replies).toHaveLength(2);
    });
    expect(replies[1]?.reply.error).toBeUndefined();
    expect(useSmabar.getState().overlayOpen).toBe(false);
  },
);

test.each([
  {
    reason: "offscreen anchor",
    y: 192,
    reordering: false,
    error: "flyout anchor is outside the viewport; reveal the bar and retry",
  },
  {
    reason: "active drag",
    y: 20,
    reordering: true,
    error: "a drag is in progress; drop the item or press Escape and retry",
  },
])(
  "the UI command cannot bypass the $reason guard",
  async ({ y, reordering, error }) => {
    const tile = document.querySelector<HTMLElement>("[data-tile-id]");
    if (tile === null) throw new Error("test tile missing");
    Object.defineProperty(tile, "getBoundingClientRect", {
      value: () => DOMRect.fromRect({ x: 20, y, width: 40, height: 20 }),
    });
    useSmabar.getState().setReordering(reordering);
    immediateFrames();

    await initCapture("bar");
    const listener = listeners.get("bar-ui-command");
    if (listener === undefined)
      throw new Error("bar-ui-command listener missing");
    listener({
      payload: {
        id: 10,
        action: "open_flyout",
        tileId: "plugin:demo:main",
      },
    });

    await vi.waitFor(() => {
      expect(replies).toHaveLength(1);
    });
    expect(replies[0]?.reply.error).toBe(error);
    expect(useSmabar.getState().openFlyout).toBeNull();
  },
);

test.each(["flyout", "overlay"])(
  "a closed %s is not a capture target",
  async (target) => {
    immediateFrames();

    await initCapture("bar");
    const listener = listeners.get("bar-capture");
    if (listener === undefined) throw new Error("bar-capture listener missing");
    listener({
      payload: {
        id: 8,
        target,
      },
    });

    await vi.waitFor(() => {
      expect(replies).toHaveLength(1);
    });
    expect(replies[0]).toMatchObject({
      reply: { id: 8, error: "unknown-target" },
    });
    expect(replies[0]?.reply.targets).not.toContain(target);
  },
);

test("UI replies include the separate flyout surface only while it is open", async () => {
  const tile = document.querySelector<HTMLElement>("[data-tile-id]");
  if (tile === null) throw new Error("test tile missing");
  tile.getBoundingClientRect = () =>
    DOMRect.fromRect({ x: 20, y: 20, width: 40, height: 20 });
  immediateFrames();
  await initCapture("bar");
  const listener = listeners.get("bar-ui-command");
  if (listener === undefined) throw new Error("UI listener missing");
  listener({
    payload: { id: 20, action: "open_flyout", tileId: "plugin:demo:main" },
  });
  await vi.waitFor(() => {
    expect(replies).toHaveLength(1);
  });
  expect(replies[0]?.reply.error).toBeUndefined();
  expect(replies[0]?.reply.targets).toContain("flyout");
  listener({ payload: { id: 21, action: "close_flyout", tileId: null } });
  await vi.waitFor(() => {
    expect(replies).toHaveLength(2);
  });
  expect(replies[1]?.reply.targets).not.toContain("flyout");
});

test("a release cancels a capture that is still preparing", async () => {
  document.body.innerHTML = '<div data-capture="flyout">content</div>';
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", () => 1);

  await initCapture("bar");
  const capture = listeners.get("bar-capture");
  const release = listeners.get("bar-capture-release");
  if (capture === undefined || release === undefined) {
    throw new Error("capture listeners missing");
  }
  capture({
    payload: {
      id: 9,
      target: "flyout",
    },
  });
  release({ payload: 9 });
  await vi.advanceTimersByTimeAsync(200);

  expect(replies).toHaveLength(0);
});
