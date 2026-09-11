// @vitest-environment happy-dom
import { beforeEach, expect, test, vi } from "vitest";
import { initSurface, reportNotificationMeasure } from "./surface";
import { useSmabar } from "../store/bar";

const { invokeMock, skipMock, listeners } = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  invokeMock: vi.fn<(command: string, args?: unknown) => Promise<unknown>>(() =>
    Promise.resolve(),
  ),
  skipMock: vi.fn<(surface: string, generation: number) => Promise<void>>(() =>
    Promise.resolve(),
  ),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, listener: (event: { payload: unknown }) => void) => {
    listeners.set(name, listener);
    return Promise.resolve(() => undefined);
  },
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn() }));
vi.mock("./inputShape", () => ({ requestBarGeometry: vi.fn() }));
vi.mock("./log", () => ({ reportError: vi.fn() }));
vi.mock("./overlay", () => ({
  hasLayout: (size: { width: number; height: number }) =>
    size.width > 0 && size.height > 0,
  skipUnmeasured: skipMock,
}));

beforeEach(() => {
  invokeMock.mockClear();
  skipMock.mockClear();
  listeners.clear();
  useSmabar.setState(useSmabar.getInitialState(), true);
});

test("bar reload restores Settings visibility and newer close events beat the snapshot", async () => {
  const context = {
    role: "bar",
    workAreaWidth: 1920,
    workAreaHeight: 1040,
    settingsOpen: true,
  };
  invokeMock.mockResolvedValueOnce(context);
  await initSurface("bar");
  expect(useSmabar.getState().settingsOpen).toBe(true);

  let resolveContext: ((context: unknown) => void) | undefined;
  invokeMock.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveContext = resolve;
      }),
  );
  const initialized = initSurface("bar");
  await vi.waitFor(() => {
    expect(resolveContext).toBeDefined();
  });
  listeners.get("settings-open-changed")?.({ payload: false });
  resolveContext?.(context);
  await initialized;
  expect(useSmabar.getState().settingsOpen).toBe(false);
  listeners.get("settings-open-changed")?.({ payload: true });
  expect(useSmabar.getState().settingsOpen).toBe(true);
});

test("a notification measure with an unlaid-out surface is skipped", async () => {
  await reportNotificationMeasure({
    popup: { width: 0, height: 0 },
    notice: null,
    edgeInset: 20,
    gap: 12,
  });
  expect(invokeMock).not.toHaveBeenCalled();
  expect(skipMock).toHaveBeenCalledWith("notification", 0);
});

test("absent surfaces stay null and laid-out ones are forwarded", async () => {
  const measure = {
    popup: null,
    notice: { width: 240, height: 48 },
    edgeInset: 20,
    gap: 12,
  };
  await reportNotificationMeasure(measure);
  expect(invokeMock).toHaveBeenCalledWith("set_notification_measure", {
    measure,
  });
});
