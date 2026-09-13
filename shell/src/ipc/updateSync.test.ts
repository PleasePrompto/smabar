// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { useSmabar } from "../store/bar";
import type { UpdateInfo } from "../store/types";
import {
  hasAppUpdate,
  initUpdateSync,
  requestUpdate,
  showUpdateNotification,
} from "./updateSync";

const { listeners, emit, install, check, stage, open, log } = vi.hoisted(
  () => ({
    listeners: new Map<string, (event: { payload: unknown }) => void>(),
    emit: vi.fn(() => Promise.resolve()),
    install: vi.fn(() => Promise.resolve()),
    check: vi.fn(() => Promise.resolve()),
    stage: vi.fn(() => Promise.resolve()),
    open: vi.fn(() => Promise.resolve()),
    log: vi.fn(),
  }),
);
vi.mock("@tauri-apps/api/event", () => ({
  emitTo: emit,
  listen: (name: string, listener: (event: { payload: unknown }) => void) => {
    listeners.set(name, listener);
    return Promise.resolve(() => {
      listeners.delete(name);
    });
  },
}));
vi.mock("./update", () => ({ installUpdate: install, checkUpdate: check }));
vi.mock("./surface", () => ({
  stageNotificationUpdate: stage,
  openSettings: open,
}));
vi.mock("./log", () => ({ reportError: log }));

const offer: UpdateInfo = {
  version: "1.2.0",
  notes: null,
  date: null,
  installer: "system",
};
let stop: (() => void) | undefined;
beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  useSmabar.setState(useSmabar.getInitialState(), true);
  useSmabar.setState({
    updateChannel: "app",
    updateOffer: offer,
    updateStatus: { state: "available", ...offer },
  });
});
afterEach(() => {
  stop?.();
  stop = undefined;
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

function snapshot(revision: number, version: string) {
  return {
    revision,
    updateChannel: "app",
    updateOffer: { ...offer, version },
    updateStatus: { state: "available", ...offer, version },
    dismissedUpdateVersion: null,
  };
}

test("dismissal survives checks of the same release, keeps the badge and allows a newer release", async () => {
  expect(showUpdateNotification(useSmabar.getState())).toBe(true);
  await requestUpdate({ action: "dismiss", version: offer.version });
  useSmabar.setState({ updateStatus: { state: "checking" } });
  useSmabar.setState({
    updateOffer: { ...offer },
    updateStatus: { state: "available", ...offer },
  });
  expect(showUpdateNotification(useSmabar.getState())).toBe(false);
  expect(hasAppUpdate(useSmabar.getState())).toBe(true);
  useSmabar.setState({ updateOffer: { ...offer, version: "1.3.0" } });
  expect(showUpdateNotification(useSmabar.getState())).toBe(true);
  useSmabar.setState(useSmabar.getInitialState(), true);
  useSmabar.setState({ updateChannel: "app", updateOffer: offer });
  expect(showUpdateNotification(useSmabar.getState())).toBe(true);
});

test("host notifications ignore plugin popup mute but respect the legal gate, channel and active install", () => {
  useSmabar.setState({
    popups: { ...useSmabar.getState().popups, enabled: false },
  });
  expect(showUpdateNotification(useSmabar.getState())).toBe(true);
  useSmabar.setState({ legalRequired: true });
  expect(showUpdateNotification(useSmabar.getState())).toBe(false);
  useSmabar.setState({ legalRequired: false, updateChannel: "store" });
  expect(showUpdateNotification(useSmabar.getState())).toBe(false);
  expect(hasAppUpdate(useSmabar.getState())).toBe(false);
  useSmabar.setState({
    updateChannel: "app",
    updateStatus: {
      state: "downloading",
      version: offer.version,
      received: 0,
      total: null,
    },
  });
  expect(showUpdateNotification(useSmabar.getState())).toBe(false);
  useSmabar.setState({
    updateStatus: { state: "failed", phase: "install", message: "offline" },
  });
  expect(showUpdateNotification(useSmabar.getState())).toBe(true);
});

test("only the current offer can install; notifications open the update section", async () => {
  await requestUpdate({ action: "install", version: "9.0.0" });
  expect(install).not.toHaveBeenCalled();
  await requestUpdate({ action: "install", version: offer.version });
  expect(install).toHaveBeenCalledWith(offer.version);
  expect(open).toHaveBeenCalledWith("system/updates");
  useSmabar.setState({ updateOffer: { ...offer, installer: null } });
  await requestUpdate({ action: "install", version: offer.version });
  expect(install).toHaveBeenCalledOnce();
});

test("native clients forward actions to the owner without invoking the installer", async () => {
  Reflect.set(window, "__TAURI_INTERNALS__", {});
  await requestUpdate({ action: "install", version: offer.version });
  expect(emit).toHaveBeenCalledWith("bar", "app-update-action", {
    action: "install",
    version: offer.version,
  });
  expect(install).not.toHaveBeenCalled();
});

test("a late-opened settings window subscribes before requesting the current state and ignores old snapshots", async () => {
  emit.mockImplementationOnce(() => {
    expect(listeners.has("app-update-settings")).toBe(true);
    listeners.get("app-update-settings")?.({ payload: snapshot(4, "1.4.0") });
    return Promise.resolve();
  });
  stop = await initUpdateSync("settings");
  expect(useSmabar.getState().updateOffer?.version).toBe("1.4.0");
  listeners.get("app-update-settings")?.({ payload: snapshot(3, "1.3.0") });
  expect(useSmabar.getState().updateOffer?.version).toBe("1.4.0");
});

test("notification staging cannot restore an older offer after a newer event", async () => {
  let finish: (() => void) | undefined;
  stage.mockImplementationOnce(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );
  stop = await initUpdateSync("notifications");
  const receive = listeners.get("app-update-notifications");
  receive?.({ payload: snapshot(1, "1.3.0") });
  receive?.({ payload: snapshot(2, "1.4.0") });
  await Promise.resolve();
  finish?.();
  await Promise.resolve();
  expect(useSmabar.getState().updateOffer?.version).toBe("1.4.0");
});

test("the owner publishes changes and rejects malformed action events", async () => {
  stop = await initUpdateSync("bar");
  useSmabar.setState({ dismissedUpdateVersion: offer.version });
  expect(emit).toHaveBeenCalledWith(
    "notifications",
    "app-update-notifications",
    expect.objectContaining({ dismissedUpdateVersion: offer.version }),
  );
  listeners.get("app-update-action")?.({
    payload: { action: "install", version: 3 },
  });
  await Promise.resolve();
  expect(log).toHaveBeenCalledWith(expect.any(Error));
  expect(install).not.toHaveBeenCalled();
});

test("download byte progress goes only to Settings while the notification is hidden", async () => {
  stop = await initUpdateSync("bar");
  useSmabar.setState({
    updateStatus: {
      state: "downloading",
      version: offer.version,
      received: 0,
      total: 100,
    },
  });
  emit.mockClear();
  useSmabar.setState({
    updateStatus: {
      state: "downloading",
      version: offer.version,
      received: 50,
      total: 100,
    },
  });
  expect(emit).toHaveBeenCalledOnce();
  expect(emit).toHaveBeenCalledWith(
    "settings",
    "app-update-settings",
    expect.objectContaining({
      updateStatus: {
        state: "downloading",
        version: offer.version,
        received: 50,
        total: 100,
      },
    }),
  );
});
