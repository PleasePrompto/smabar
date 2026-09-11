// @vitest-environment happy-dom
import { act, StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../../store/bar";
import { SystemTab } from "./SystemTab";
import type { AutostartStatus } from "./useAutostart";

const { callMock, listenMock, reportMock } = vi.hoisted(() => ({
  callMock: vi.fn(),
  listenMock: vi.fn(),
  reportMock: vi.fn(),
}));
vi.mock("../../ipc/call", () => ({ call: callMock }));
vi.mock("../../ipc/log", () => ({ reportError: reportMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

let container: HTMLDivElement;
let root: Root;
let osStatus: AutostartStatus;
let receive: ((event: { payload: AutostartStatus }) => void) | undefined;

beforeEach(() => {
  osStatus = { state: "ready", registered: true };
  receive = undefined;
  callMock.mockImplementation(
    (command: string, args?: { enabled: boolean }) => {
      if (command === "get_autostart_status") return Promise.resolve(osStatus);
      if (command === "set_autostart") {
        osStatus = { state: "ready", registered: args?.enabled ?? false };
        return Promise.resolve(osStatus);
      }
      if (command === "update_config") return Promise.resolve();
      return new Promise(() => undefined);
    },
  );
  listenMock.mockImplementation((event: string, callback: typeof receive) => {
    if (event === "autostart-changed") receive = callback;
    return Promise.resolve(vi.fn());
  });
  useSmabar.setState(useSmabar.getInitialState(), true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  vi.clearAllMocks();
});

function toggle(): HTMLInputElement {
  const input = container.querySelector<HTMLInputElement>(
    '[aria-label="Start at login"]',
  );
  if (input === null) throw new Error("autostart switch missing");
  return input;
}

function button(label: string): HTMLButtonElement {
  const found = [
    ...container.querySelectorAll<HTMLButtonElement>("button"),
  ].find((element) => element.textContent === label);
  if (found === undefined) throw new Error(`button missing: ${label}`);
  return found;
}

async function interact(run: () => void) {
  await act(async () => {
    run();
    await Promise.resolve();
  });
}

async function render() {
  await interact(() => {
    root.render(
      <StrictMode>
        <SystemTab />
      </StrictMode>,
    );
  });
}

function pendingStatus() {
  let resolve: (status: AutostartStatus) => void = () => {
    throw new Error("pending status resolver was not initialized");
  };
  const promise = new Promise<AutostartStatus>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

test("loads OS status, confirms changes, refreshes external opt-outs and resets to on", async () => {
  await render();
  expect(toggle().checked).toBe(true);
  await interact(() => {
    toggle().click();
  });
  expect(callMock).toHaveBeenCalledWith("set_autostart", { enabled: false });
  expect(toggle().checked).toBe(false);
  await interact(() => {
    button("Reset to defaults").click();
  });
  expect(callMock).toHaveBeenCalledWith("set_autostart", { enabled: true });
  expect(toggle().checked).toBe(true);
  osStatus = { state: "ready", registered: false };
  await interact(() => window.dispatchEvent(new Event("focus")));
  expect(toggle().checked).toBe(false);
});

test("loading and pending writes lock the switch without displaying optimistic success", async () => {
  const read = pendingStatus();
  const write = pendingStatus();
  callMock.mockImplementation((command: string) =>
    command === "get_autostart_status"
      ? read.promise
      : command === "set_autostart"
        ? write.promise
        : new Promise(() => undefined),
  );
  await render();
  expect(toggle().disabled).toBe(true);
  await interact(() => {
    read.resolve(osStatus);
  });
  await interact(() => {
    toggle().click();
  });
  expect(toggle().disabled).toBe(true);
  expect(toggle().checked).toBe(true);
  await interact(() => {
    write.resolve({ state: "ready", registered: false });
  });
  expect(toggle().disabled).toBe(false);
  expect(toggle().checked).toBe(false);
});

test("backend failures retain the real state and can be retried", async () => {
  await render();
  callMock.mockImplementation((command: string) => {
    if (command === "set_autostart")
      return Promise.resolve({ state: "failed", registered: true });
    if (command === "get_autostart_status") return Promise.resolve(osStatus);
    return new Promise(() => undefined);
  });
  await interact(() => {
    toggle().click();
  });
  expect(toggle().checked).toBe(true);
  expect(container.querySelector('[role="alert"]')?.textContent).toContain(
    "Autostart could not be confirmed",
  );
  await interact(() => {
    button("Refresh status").click();
  });
  expect(container.querySelector('[role="alert"]')).toBeNull();
});

test("a failed status read offers a retry instead of an active unchecked switch", async () => {
  callMock.mockImplementation((command: string) =>
    command === "get_autostart_status"
      ? Promise.reject(new Error("IPC unavailable"))
      : new Promise(() => undefined),
  );
  await render();
  expect(toggle().disabled).toBe(true);
  expect(reportMock).toHaveBeenCalled();
  expect(container.querySelector('[role="alert"]')).not.toBeNull();
  callMock.mockImplementation(() => Promise.resolve(osStatus));
  await interact(() => {
    button("Refresh status").click();
  });
  expect(toggle().disabled).toBe(false);
  expect(toggle().checked).toBe(true);
});

test("development registration is unavailable, including system reset", async () => {
  osStatus = { state: "unavailable" };
  await render();
  expect(toggle().disabled).toBe(true);
  expect(container.textContent).toContain(
    "Development builds leave your login settings untouched",
  );
  await interact(() => {
    button("Reset to defaults").click();
  });
  expect(
    callMock.mock.calls.some(([command]) => command === "set_autostart"),
  ).toBe(false);
});

test("tray events win over an older in-flight status read", async () => {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
  const read = pendingStatus();
  callMock.mockImplementation((command: string) =>
    command === "get_autostart_status"
      ? read.promise
      : new Promise(() => undefined),
  );
  await render();
  await interact(() =>
    receive?.({ payload: { state: "ready", registered: false } }),
  );
  await interact(() => {
    read.resolve({ state: "ready", registered: true });
  });
  expect(toggle().checked).toBe(false);
  expect(toggle().disabled).toBe(false);
});
