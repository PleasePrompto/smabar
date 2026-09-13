// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";

type Listener = (event: { payload: unknown }) => void;

const listeners = new Map<string, Listener>();
let result: unknown = null;
let fail = false;
let settleInstall: {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
} | null = null;

vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, listener: Listener) => {
    listeners.set(name, listener);
    return Promise.resolve(() => undefined);
  },
}));

vi.mock("./call", () => ({
  call: (command: string) => {
    if (command === "install_update") {
      // Stays pending until the test settles it — like a real download.
      return new Promise((resolve, reject) => {
        settleInstall = { resolve, reject };
      });
    }
    return fail
      ? Promise.reject(new Error("offline"))
      : Promise.resolve(result);
  },
}));

import { useSmabar } from "../store/bar";
import {
  checkUpdate,
  FIRST_CHECK_MS,
  initUpdateEvents,
  initUpdates,
  installUpdate,
  scheduleUpdateChecks,
} from "./update";

const release = {
  version: "0.2.0",
  notes: "Notes",
  date: null,
  installer: "system",
};

function progress(received: number, total: number | null, finished = false) {
  listeners.get("update-progress")?.({
    payload: { received, total, finished },
  });
}

beforeEach(async () => {
  result = null;
  fail = false;
  settleInstall = null;
  listeners.clear();
  useSmabar.setState({ updateChannel: "app" });
  useSmabar.setState({ updateOffer: null, dismissedUpdateVersion: null });
  useSmabar.getState().setUpdateStatus({ state: "idle" });
  await initUpdateEvents();
  vi.useFakeTimers();
});

test("Store and unknown channels never subscribe, check or install", async () => {
  for (const channel of ["store", "unknown"]) {
    listeners.clear();
    result = { updateChannel: channel };
    await initUpdates(true);
    await checkUpdate();
    await installUpdate("1.0.1");
    expect(listeners.has("update-progress")).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
    expect(settleInstall).toBeNull();
    expect(useSmabar.getState().updateStatus.state).toBe("idle");
  }
});

test("channel lookup failure does not reject shell startup", async () => {
  listeners.clear();
  fail = true;
  await expect(initUpdates(true)).resolves.toBeUndefined();
  expect(useSmabar.getState().updateChannel).toBeNull();
  expect(listeners.size).toBe(0);
});

afterEach(() => {
  vi.useRealTimers();
});

test("a newer release is 'available', null means 'current'", async () => {
  result = release;
  await checkUpdate();
  expect(useSmabar.getState().updateStatus).toEqual({
    state: "available",
    ...release,
  });
  result = null;
  await checkUpdate();
  expect(useSmabar.getState().updateStatus).toEqual({ state: "current" });
});

test("a failed check is a state carrying the reason, never a throw", async () => {
  fail = true;
  await checkUpdate();
  const status = useSmabar.getState().updateStatus;
  expect(status.state === "failed" && status.phase).toBe("check");
  expect(status.state === "failed" && status.message).toContain("offline");
});

test("a failed background check retains the confirmed offer for badges and retry", async () => {
  result = release;
  await checkUpdate();
  fail = true;
  await checkUpdate();
  expect(useSmabar.getState().updateOffer).toEqual(release);
  expect(useSmabar.getState().updateStatus.state).toBe("failed");
  fail = false;
  result = null;
  await checkUpdate();
  expect(useSmabar.getState().updateOffer).toBeNull();
});

test("concurrent checks do not start a second request", async () => {
  result = release;
  const first = checkUpdate();
  expect(useSmabar.getState().updateStatus.state).toBe("checking");
  result = null;
  await checkUpdate();
  await first;
  expect(useSmabar.getState().updateOffer).toEqual(release);
});

test("the first background check waits for the bar to come up", async () => {
  scheduleUpdateChecks();
  expect(useSmabar.getState().updateStatus.state).toBe("idle");
  await vi.advanceTimersByTimeAsync(FIRST_CHECK_MS);
  expect(useSmabar.getState().updateStatus.state).toBe("current");
});

test("an install walks downloading → installing → handedOff", async () => {
  const install = installUpdate("0.2.0");
  expect(useSmabar.getState().updateStatus).toEqual({
    state: "downloading",
    version: "0.2.0",
    received: 0,
    total: null,
  });
  progress(1_000_000, 5_000_000);
  expect(useSmabar.getState().updateStatus).toEqual({
    state: "downloading",
    version: "0.2.0",
    received: 1_000_000,
    total: 5_000_000,
  });
  progress(5_000_000, 5_000_000, true);
  expect(useSmabar.getState().updateStatus).toEqual({
    state: "installing",
    version: "0.2.0",
  });
  settleInstall?.resolve({ path: "/tmp/smabar_0.2.0_amd64.deb", opened: true });
  await install;
  expect(useSmabar.getState().updateStatus).toEqual({
    state: "handedOff",
    version: "0.2.0",
    path: "/tmp/smabar_0.2.0_amd64.deb",
    opened: true,
  });
});

test("a rejected install fails with its phase and ignores late progress", async () => {
  const install = installUpdate("0.2.0");
  settleInstall?.reject(new Error("signature mismatch"));
  await install;
  const status = useSmabar.getState().updateStatus;
  expect(status.state === "failed" && status.phase).toBe("install");
  expect(status.state === "failed" && status.message).toContain("signature");
  progress(4_000_000, 5_000_000);
  expect(useSmabar.getState().updateStatus.state).toBe("failed");
});

test("checks and a second install stand back while a download runs", async () => {
  const install = installUpdate("0.2.0");
  result = release;
  await checkUpdate();
  await installUpdate("0.2.0");
  expect(useSmabar.getState().updateStatus.state).toBe("downloading");
  settleInstall?.resolve({ path: "/tmp/x.deb", opened: false });
  await install;
  expect(useSmabar.getState().updateStatus.state).toBe("handedOff");
});
