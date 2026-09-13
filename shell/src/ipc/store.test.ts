// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";

type Listener = (event: { payload: unknown }) => void;

const listeners = new Map<string, Listener>();

const { callMock, reportErrorMock } = vi.hoisted(() => ({
  callMock: vi.fn(),
  reportErrorMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, listener: Listener) => {
    listeners.set(name, listener);
    return Promise.resolve(() => {
      listeners.delete(name);
    });
  },
}));
vi.mock("./call", () => ({ call: callMock }));
vi.mock("./log", () => ({ reportError: reportErrorMock }));

import { useSmabar } from "../store/bar";
import {
  installStorePlugin,
  installStoreTheme,
  onStoreChanged,
  onStoreProgress,
  refreshCommunityBadge,
  storeDetail,
} from "./store";

const update = {
  fromVersion: "1.0.0",
  toVersion: "1.1.0",
  contentChanged: false,
};

beforeEach(() => {
  listeners.clear();
  callMock.mockReset();
  reportErrorMock.mockReset();
  useSmabar.setState(useSmabar.getInitialState(), true);
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

test("the badge count is the number of listed updates", async () => {
  callMock.mockResolvedValue({
    entries: [{ update }, { update: null }, { update }],
  });
  await refreshCommunityBadge();
  expect(callMock).toHaveBeenCalledWith("store_overview", undefined);
  expect(useSmabar.getState().communityUpdates).toHaveLength(2);
});

test("a failed overview leaves the count alone and is logged, never thrown", async () => {
  callMock.mockResolvedValue({ entries: [{ update }, { update }, { update }] });
  await refreshCommunityBadge();
  callMock.mockRejectedValue("catalog signature invalid");
  await expect(refreshCommunityBadge()).resolves.toBeUndefined();
  expect(useSmabar.getState().communityUpdates).toHaveLength(3);
  expect(reportErrorMock).toHaveBeenCalledWith("catalog signature invalid");
});

test("a stale overview cannot restore a marker after a newer response removed it", async () => {
  let finish: ((overview: unknown) => void) | undefined;
  callMock.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const first = refreshCommunityBadge();
  callMock.mockResolvedValue({ entries: [] });
  await refreshCommunityBadge();
  finish?.({ entries: [{ update }] });
  await first;
  expect(useSmabar.getState().communityUpdates).toEqual([]);
});

test("commands carry camelCase arguments exactly as the core expects", async () => {
  callMock.mockResolvedValue({ entries: [] });
  await installStorePlugin("pomodoro", "2.0.1", { confirmModified: true });
  expect(callMock).toHaveBeenCalledWith("store_install_plugin", {
    id: "pomodoro",
    expectedVersion: "2.0.1",
    confirmModified: true,
  });
  await installStoreTheme("nord", "1.0.2");
  expect(callMock).toHaveBeenCalledWith("store_install_theme", {
    name: "nord",
    expectedVersion: "1.0.2",
  });
  await storeDetail("theme", "nord");
  expect(callMock).toHaveBeenCalledWith("store_detail", {
    kind: "theme",
    id: "nord",
  });
});

test("outside the Tauri window the events are no-ops", async () => {
  const stop = await onStoreChanged(() => undefined);
  expect(listeners.size).toBe(0);
  expect(() => {
    stop();
  }).not.toThrow();
});

test("inside the Tauri window the payload reaches the handler until unlistened", async () => {
  Reflect.set(window, "__TAURI_INTERNALS__", {});
  const changed = vi.fn();
  const progressed = vi.fn();
  const stopChanged = await onStoreChanged(changed);
  await onStoreProgress(progressed);
  listeners.get("store-changed")?.({
    payload: { reason: "refresh", catalogState: "fresh" },
  });
  listeners.get("store-progress")?.({
    payload: {
      kind: "plugin",
      id: "pomodoro",
      phase: "downloading",
      received: 10,
      total: 100,
    },
  });
  expect(changed).toHaveBeenCalledWith({
    reason: "refresh",
    catalogState: "fresh",
  });
  expect(progressed).toHaveBeenCalledWith(
    expect.objectContaining({ id: "pomodoro", phase: "downloading" }),
  );
  stopChanged();
  expect(listeners.has("store-changed")).toBe(false);
  expect(listeners.has("store-progress")).toBe(true);
});
