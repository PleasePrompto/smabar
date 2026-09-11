// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";
import { useSmabar } from "../store/bar";
import { initManagedPopups } from "./managedPopups";

const { invokeMock, stageMock, listeners } = vi.hoisted(() => ({
  invokeMock: vi.fn<(command: string, args?: unknown) => Promise<unknown>>(),
  stageMock: vi.fn<() => Promise<void>>(),
  listeners: new Map<string, () => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, callback: () => void) => {
    listeners.set(name, callback);
    return Promise.resolve(() => listeners.delete(name));
  },
}));
vi.mock("./surface", () => ({ stageNotificationUpdate: stageMock }));
vi.mock("./log", () => ({ reportError: vi.fn() }));

test("snapshots preserve legacy popups and unchanged snapshots never conceal them", async () => {
  useSmabar.setState(useSmabar.getInitialState(), true);
  useSmabar
    .getState()
    .enqueuePopup({ pluginId: "legacy", tileId: "main", html: "Legacy" });
  stageMock.mockResolvedValue();
  const item = {
    instanceId: 7,
    pluginId: "todos",
    tileId: "main",
    html: "First",
  };
  let snapshot: (typeof item)[] = [];
  invokeMock.mockImplementation(() => Promise.resolve(snapshot));
  await initManagedPopups();
  expect(stageMock).not.toHaveBeenCalled();

  snapshot = [item];
  listeners.get("managed-popups-changed")?.();
  await vi.waitFor(() => {
    expect(useSmabar.getState().popupQueue.visible).toHaveLength(2);
  });
  expect(stageMock).toHaveBeenCalledOnce();
  const id = useSmabar.getState().popupQueue.visible[1]?.id;
  let finish: ((value: (typeof item)[]) => void) | undefined;
  invokeMock.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  listeners.get("managed-popups-changed")?.();
  snapshot = [{ ...item, html: "Latest" }];
  listeners.get("managed-popups-changed")?.();
  finish?.([{ ...item, html: "Stale" }]);
  await vi.waitFor(() => {
    expect(useSmabar.getState().popupQueue.visible[1]?.html).toBe("Latest");
  });
  expect(useSmabar.getState().popupQueue.visible[1]?.id).toBe(id);
  expect(stageMock).toHaveBeenCalledTimes(2);

  snapshot = [];
  listeners.get("managed-popups-changed")?.();
  await vi.waitFor(() => {
    expect(useSmabar.getState().popupQueue.visible).toHaveLength(1);
  });
  expect(useSmabar.getState().popupQueue.visible[0]?.html).toBe("Legacy");
  expect(invokeMock).not.toHaveBeenCalledWith(
    "popup_event_report",
    expect.anything(),
  );
});
