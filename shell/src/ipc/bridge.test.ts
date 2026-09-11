// @vitest-environment happy-dom
import { beforeEach, expect, test, vi } from "vitest";

import {
  useSmabar,
  type LayoutConfig,
  type ShortcutsState,
} from "../store/bar";
import { initBridge, needsKeyboardFocus, runtimeFailureNotice } from "./bridge";
import {
  nativePointerIsInside,
  pushNativePointerSample,
} from "../components/bar/useAutohide";

type Listener = (event: { payload: unknown }) => void;

const { invokeMock, listeners } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listeners: new Map<string, Listener>(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, listener: Listener) => {
    listeners.set(name, listener);
    return Promise.resolve(() => undefined);
  }),
}));

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  useSmabar.setState(useSmabar.getInitialState(), true);
});

test("keyboard-capable plugin controls request focus for the dock window", () => {
  document.body.innerHTML = `
    <input id="input">
    <select id="select"><option>One</option></select>
    <textarea id="textarea"></textarea>
    <iframe id="embed" title="Player"></iframe>
    <button id="custom-select" role="combobox"></button>
    <div id="carousel" data-carousel tabindex="0"></div>
    <div role="menu"><button id="menu-item">Item</button></div>
    <div id="plain"></div>
  `;

  for (const id of [
    "input",
    "select",
    "textarea",
    "embed",
    "custom-select",
    "carousel",
    "menu-item",
  ]) {
    expect(needsKeyboardFocus(document.getElementById(id)), id).toBe(true);
  }
  expect(needsKeyboardFocus(document.getElementById("plain"))).toBe(false);
});

test("only the transition INTO failed produces a runtime toast", () => {
  const failed = { state: "failed", kind: "offline" } as const;
  const installing = { state: "installing" } as const;
  const ready = { state: "ready" } as const;

  expect(runtimeFailureNotice(null, failed)).toBe(
    "settings.system.runtimeFailedNotice",
  );
  expect(runtimeFailureNotice(installing, failed)).toBe(
    "settings.system.runtimeFailedNotice",
  );
  // A repeated failure event must not re-toast.
  expect(runtimeFailureNotice(failed, failed)).toBeNull();
  expect(runtimeFailureNotice(installing, ready)).toBeNull();
  expect(runtimeFailureNotice(null, installing)).toBeNull();
});

test("startup events and the newest shortcut refresh win asynchronous races", async () => {
  const initial = useSmabar.getInitialState();
  const oldLayout: LayoutConfig = { ...initial.layout, position: "bottom" };
  const newLayout: LayoutConfig = { ...oldLayout, position: "top" };
  const uiRequest = deferred({
    language: initial.language,
    layout: oldLayout,
    zOrder: initial.zOrder,
    appearance: initial.appearance,
    popups: initial.popups,
    settingsWindow: initial.settingsWindow,
    pluginsHidden: initial.pluginsHidden,
    pluginsDeactivated: initial.pluginsDeactivated,
    effects: initial.effects,
    shortcuts: initial.shortcuts,
    locale: {},
    theme: {},
    themeName: initial.theme,
    pluginOrder: initial.pluginOrder,
    plugins: {},
    dataRoot: "/data",
    embedRoot: "http://127.0.0.1:1",
  });
  const shortcutRequests: ReturnType<typeof deferred<ShortcutsState>>[] = [];
  invokeMock.mockImplementation((command: string) => {
    switch (command) {
      case "get_ui_state":
        return uiRequest.promise;
      case "get_plugins":
      case "get_plugin_ui":
        return Promise.resolve([]);
      case "get_runtime_status":
        return Promise.resolve({ state: "ready" });
      case "get_shortcuts": {
        const request = deferred<ShortcutsState>();
        shortcutRequests.push(request);
        return request.promise;
      }
      default:
        return Promise.reject(new Error(`unexpected command: ${command}`));
    }
  });

  const initialized = initBridge("bar");
  await vi.waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith("get_ui_state");
  });
  emit("layout-changed", { layout: newLayout });
  uiRequest.resolve();
  await initialized;

  // X11 and Windows watchdog samples must invalidate cached DOM hover just
  // like Wayland crossing events, even when the outside sample is deduplicated.
  pushNativePointerSample(937, 6);
  emit("bar-pointer-sample", null);
  expect(nativePointerIsInside()).toBe(false);
  emit("bar-pointer-sample", [937, 6]);
  expect(nativePointerIsInside()).toBe(true);

  expect(useSmabar.getState().layout).toEqual(newLayout);

  emit("shortcuts-changed", undefined);
  emit("shortcuts-changed", undefined);
  expect(shortcutRequests).toHaveLength(2);
  const older = shortcuts("older");
  const newer = shortcuts("newer");
  shortcutRequests[1]?.resolve(newer);
  await Promise.resolve();
  shortcutRequests[0]?.resolve(older);
  await Promise.resolve();

  expect(useSmabar.getState().shortcuts).toEqual(newer);
});

interface Deferred<T> {
  promise: Promise<T>;
  resolve(value?: T): void;
}

function deferred<T>(value?: T): Deferred<T> {
  let settle: ((value: T) => void) | undefined;
  const promise = new Promise<T>((resolve) => {
    settle = resolve;
  });
  return {
    promise,
    resolve(next = value) {
      if (next === undefined) throw new Error("deferred value is missing");
      settle?.(next);
    },
  };
}

function emit(name: string, payload: unknown): void {
  const listener = listeners.get(name);
  if (listener === undefined) throw new Error(`missing ${name} listener`);
  listener({ payload });
}

function shortcuts(id: string): ShortcutsState {
  const initial = useSmabar.getInitialState().shortcuts;
  return {
    ...initial,
    pinned: [{ id, label: id, icons: [], separator: false }],
    entries: [{ id }],
  };
}
