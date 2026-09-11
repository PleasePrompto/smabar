// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { fixtureCall } from "../../ipc/fixture";
import { useSmabar, type InstalledPlugin } from "../../store/bar";
import { registerTile, unregisterPluginTiles } from "../registry";
import { typeInput } from "./ThemeManager.testHarness";
import { AudioGroup } from "./AudioGroup";
import { PluginsTab } from "./PluginsTab";
import type { AudioConfig } from "./useAudioSettings";

const { callMock, listenMock } = vi.hoisted(() => ({
  callMock:
    vi.fn<
      (command: string, args?: Record<string, unknown>) => Promise<unknown>
    >(),
  listenMock: vi.fn(),
}));
vi.mock("../../ipc/call", () => ({ call: callMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

let container: HTMLDivElement;
let root: Root;
let plugins: InstalledPlugin[];
let audio: AudioConfig;
let fail: string | null;
const events = new Map<string, (event: { payload: AudioConfig }) => void>();
const stopped = vi.fn();

function plugin(
  id: string,
  extra: Partial<InstalledPlugin> = {},
): InstalledPlugin {
  return {
    id,
    name: id,
    description: null,
    settingsSchema: null,
    tiles: [{ id: "main", name: id }],
    status: "running",
    origin: "base",
    version: "1.2.3",
    modified: false,
    update: null,
    blocked: null,
    ...extra,
  };
}

function syncRegistry() {
  for (const entry of plugins) {
    unregisterPluginTiles(entry.id);
    if (
      useSmabar.getState().pluginsDeactivated.includes(entry.id) ||
      entry.status !== "running"
    )
      continue;
    for (const tile of entry.tiles)
      registerTile({
        id: `plugin:${entry.id}:${tile.id}`,
        pluginId: entry.id,
        tile,
        meta: { name: tile.name },
      });
  }
  useSmabar.getState().bumpRegistryVersion();
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("reportError", vi.fn());
  useSmabar.setState(useSmabar.getInitialState(), true);
  fail = null;
  plugins = [
    plugin("Clock", {
      description: "Time in your cities",
      iconDataUrl: "data:image/png;base64,aWNvbg==",
      settingsSchema: {
        type: "object",
        properties: {
          seconds: { type: "boolean", title: "Show seconds", default: true },
        },
      },
    }),
    plugin("Media", { origin: "community", update: "1.3.0", modified: true }),
    plugin("Off", {
      status: "deactivated",
      origin: "user",
      settingsSchema: {
        type: "object",
        properties: { label: { type: "string", default: "Home" } },
      },
    }),
    plugin("Broken", {
      name: null,
      status: "failed",
      tiles: [],
      version: null,
      error: "private native path",
    }),
  ];
  useSmabar.getState().setPluginsDeactivated(["Off"]);
  syncRegistry();
  audio = { volume: 80, muted: false, notificationSounds: true, plugins: {} };
  events.clear();
  stopped.mockClear();
  listenMock.mockClear();
  listenMock.mockImplementation(
    (event: string, handler: (event: { payload: AudioConfig }) => void) => {
      events.set(event, handler);
      return Promise.resolve(stopped);
    },
  );
  callMock.mockReset();
  callMock.mockImplementation(
    async (command: string, args?: Record<string, unknown>) => {
      await Promise.resolve();
      if (command === fail) throw new Error("Controlled failure");
      if (command === "list_plugins") return structuredClone(plugins);
      if (command === "get_audio_settings") return structuredClone(audio);
      if (command === "remove_plugin") {
        const id = String(args?.pluginId);
        unregisterPluginTiles(id);
        plugins = plugins.filter((entry) => entry.id !== id);
        useSmabar.getState().bumpRegistryVersion();
        return null;
      }
      if (command === "update_config") {
        const path = String(args?.path);
        if (path.startsWith("audio.")) {
          const parts = path.split(".");
          const level =
            parts[1] === "plugins"
              ? (audio.plugins[parts[2] ?? ""] ??= {
                  volume: 100,
                  muted: false,
                })
              : audio;
          if (path.endsWith(".volume")) level.volume = Number(args?.value);
          if (path.endsWith(".muted")) level.muted = Boolean(args?.value);
          if (path === "audio.notificationSounds")
            audio.notificationSounds = Boolean(args?.value);
          return null;
        }
        const result = fixtureCall(command, args);
        if (path === "pluginsDeactivated") syncRegistry();
        return result;
      }
      if (command === "ui_log") return null;
      throw new Error(`Unexpected command: ${command}`);
    },
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  for (const id of ["Clock", "Media", "Off", "Broken", "Multi"])
    unregisterPluginTiles(id);
  container.remove();
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  vi.unstubAllGlobals();
});

async function settle(action: () => void = () => undefined) {
  await act(async () => {
    action();
    for (let turn = 0; turn < 12; turn++) await Promise.resolve();
  });
}

function card(id: string): HTMLElement {
  const match = [
    ...container.querySelectorAll<HTMLElement>(".settings-plugin-card"),
  ].find((entry) => entry.getAttribute("aria-label") === id);
  if (!match) throw new Error(`Missing card ${id}`);
  return match;
}

function button(host: HTMLElement, label: string): HTMLButtonElement {
  const match = [...host.querySelectorAll("button")]
    .filter((entry) => !entry.hidden)
    .find(
      (entry) =>
        entry.getAttribute("aria-label") === label ||
        entry.textContent === label,
    );
  if (!match) throw new Error(`Missing button ${label}`);
  return match;
}

function input(host: HTMLElement, label: string): HTMLInputElement {
  const match = host.querySelector<HTMLInputElement>(
    `input[aria-label="${label}"]`,
  );
  if (!match) throw new Error(`Missing input ${label}`);
  return match;
}

test("every installed plugin has a folded card with facts and actions, including failed and schema-free plugins", async () => {
  await settle(() => {
    root.render(<PluginsTab />);
  });
  expect(container.querySelectorAll(".settings-plugin-card")).toHaveLength(4);
  expect(card("Clock").textContent).toContain("Time in your cities");
  expect(card("Clock").textContent).toContain("Bundled");
  expect(card("Clock").textContent).toContain("v1.2.3");
  expect(
    card("Clock")
      .querySelector(".settings-plugin-icon > span")
      ?.shadowRoot?.querySelector("img")?.src,
  ).toBe("data:image/png;base64,aWNvbg==");
  expect(card("Media").textContent).toContain("Community Plugins");
  expect(card("Media").textContent).toContain("Modified locally");
  expect(card("Off").textContent).toContain("Deactivated");
  expect(input(card("Off"), "Label").value).toBe("Home");
  expect(card("Broken").textContent).toContain("Could not start");
  expect(card("Broken").textContent).not.toContain("private native path");
  for (const node of container.querySelectorAll<HTMLDetailsElement>(
    ".settings-plugin-details",
  ))
    expect(node.open).toBe(false);
  expect(button(card("Clock"), "Deactivate")).toBeDefined();
  expect(button(card("Off"), "Hide plugin")).toBeDefined();
  expect(input(card("Media"), "Volume").value).toBe("100");
  expect(
    callMock.mock.calls.filter(([command]) => command === "list_plugins"),
  ).toHaveLength(1);
  expect(
    callMock.mock.calls.filter(([command]) => command === "get_audio_settings"),
  ).toHaveLength(1);
});

test("visibility and activation stay in sync between cards and sort list without losing card state", async () => {
  await settle(() => {
    root.render(<PluginsTab />);
  });
  const clock = card("Clock");
  const details = clock.querySelector("details");
  if (!details) throw new Error("Missing details");
  details.open = true;
  await settle(() => {
    button(clock, "Hide plugin").click();
  });
  expect(useSmabar.getState().pluginsHidden).toEqual(["plugin:Clock:main"]);
  expect(
    container.querySelector('.sb-list button[aria-label="Show plugin"]'),
  ).not.toBeNull();
  expect(clock.textContent).toContain("Hidden");
  await settle(() => {
    button(clock, "Deactivate").click();
  });
  expect(useSmabar.getState().pluginsDeactivated).toContain("Clock");
  expect(card("Clock")).toBe(clock);
  expect(details.open).toBe(true);
  const list = container.querySelector<HTMLElement>(".sb-list");
  if (!list) throw new Error("Missing list");
  await settle(() => {
    button(list, "Activate").click();
  });
  expect(button(clock, "Deactivate")).toBeDefined();
  expect(input(clock, "Show seconds").checked).toBe(true);
  await settle(() => {
    input(clock, "Show seconds").click();
  });
  expect(callMock).toHaveBeenCalledWith("update_config", {
    path: "plugins.Clock.seconds",
    value: false,
  });
});

test("plugins with multiple tiles have individual visibility and shared audio, activation and deletion", async () => {
  plugins.push(
    plugin("Multi", {
      tiles: [
        { id: "a", name: "First" },
        { id: "b", name: "Second" },
      ],
    }),
  );
  syncRegistry();
  await settle(() => {
    root.render(<PluginsTab />);
  });
  const multi = card("Multi");
  expect(multi.textContent).toContain("This plugin displays 2 tiles");
  await settle(() => {
    button(multi, "Hide plugin: First").click();
  });
  expect(useSmabar.getState().pluginsHidden).toEqual(["plugin:Multi:a"]);
  expect(button(multi, "Hide plugin: Second")).toBeDefined();
  expect(multi.querySelectorAll('input[aria-label="Volume"]')).toHaveLength(1);
  expect(
    [...multi.querySelectorAll("button")].some((entry) =>
      entry.getAttribute("aria-label")?.startsWith("Deactivate —"),
    ),
  ).toBe(true);
});

test("deletion needs confirmation, Escape restores focus, failures preserve data and retry removes both views", async () => {
  await settle(() => {
    root.render(<PluginsTab />);
  });
  const clock = card("Clock");
  await settle(() => {
    button(clock, "Delete permanently").click();
  });
  expect(clock.textContent).toContain(
    "Its code, data and logs will be deleted",
  );
  expect(
    callMock.mock.calls.some(([command]) => command === "remove_plugin"),
  ).toBe(false);
  await settle(() =>
    button(clock, "Cancel").dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    ),
  );
  expect(document.activeElement).toBe(button(clock, "Delete permanently"));
  await settle(() => {
    button(clock, "Delete permanently").click();
  });
  fail = "remove_plugin";
  await settle(() => {
    button(clock, "Delete").click();
  });
  expect(clock.querySelector('[role="alert"]')?.textContent).toContain(
    "Could not complete",
  );
  expect(card("Clock")).toBe(clock);
  fail = null;
  await settle(() => {
    button(clock, "Delete").click();
  });
  expect(
    container.querySelector('.settings-plugin-card[aria-label="Clock"]'),
  ).toBeNull();
  expect(container.querySelector(".sb-list")?.textContent).not.toContain(
    "Clock",
  );
  expect(document.activeElement?.id).toBe("plugin-settings-title");
});

test("audio saves to the plugin path, preserves siblings and displays failed writes", async () => {
  await settle(() => {
    root.render(<PluginsTab />);
  });
  await settle(() => {
    typeInput(input(card("Clock"), "Volume"), "35");
  });
  expect(audio.plugins.Clock).toEqual({ volume: 35, muted: false });
  expect(audio.volume).toBe(80);
  expect(input(card("Media"), "Volume").value).toBe("100");
  fail = "update_config";
  await settle(() => {
    input(card("Clock"), "Muted").click();
  });
  expect(card("Clock").querySelector('[role="alert"]')?.textContent).toContain(
    "Could not load or save audio settings",
  );
  expect(input(card("Clock"), "Muted").checked).toBe(false);
  expect(card("Media").querySelector('[role="alert"]')).toBeNull();
  fail = null;
  await settle(() => {
    input(card("Clock"), "Muted").click();
  });
  expect(audio.plugins.Clock?.muted).toBe(true);
});

test("one audio subscription updates all cards and unsubscribes on unmount", async () => {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  await settle(() => {
    root.render(<PluginsTab />);
  });
  expect(
    listenMock.mock.calls.filter(([name]) => name === "audio-settings-changed"),
  ).toHaveLength(1);
  await settle(() =>
    events.get("audio-settings-changed")?.({
      payload: {
        ...audio,
        muted: true,
        plugins: { Clock: { volume: 20, muted: true } },
      },
    }),
  );
  expect(input(card("Clock"), "Volume").value).toBe("20");
  expect(input(card("Clock"), "Muted").checked).toBe(true);
  expect(card("Clock").textContent).toContain("master output is muted");
  await settle(() => {
    root.render(null);
  });
  expect(stopped).toHaveBeenCalledTimes(2);
});

test("load failures have a retry and system audio contains only global controls", async () => {
  fail = "list_plugins";
  await settle(() => {
    root.render(<PluginsTab />);
  });
  expect(container.querySelector('[role="alert"]')?.textContent).toContain(
    "Could not load installed plugins",
  );
  fail = null;
  await settle(() => {
    button(container, "Reload").click();
  });
  expect(card("Clock")).toBeDefined();
  await settle(() => {
    root.render(<AudioGroup />);
  });
  expect(container.textContent).toContain("smabar audio");
  expect(container.querySelectorAll('input[aria-label="Volume"]')).toHaveLength(
    1,
  );
  expect(container.textContent).not.toContain("Clock");
  await settle(() => {
    input(container, "Notification sounds").click();
  });
  expect(audio.notificationSounds).toBe(false);
});

test("a pending action blocks duplicate writes from both views", async () => {
  await settle(() => {
    root.render(<PluginsTab />);
  });
  let complete: (() => void) | undefined;
  const waiting = new Promise<void>((resolve) => {
    complete = resolve;
  });
  const original = callMock.getMockImplementation();
  if (!original) throw new Error("Missing command implementation");
  callMock.mockImplementationOnce(
    async (command: string, args?: Record<string, unknown>) => {
      await waiting;
      return original(command, args);
    },
  );
  await settle(() => {
    button(card("Clock"), "Hide plugin").click();
    button(card("Media"), "Hide plugin").click();
  });
  expect(
    callMock.mock.calls.filter(([command]) => command === "update_config"),
  ).toHaveLength(1);
  expect(card("Clock").querySelector("fieldset")?.disabled).toBe(true);
  expect(
    container.querySelector(".sb-list fieldset")?.hasAttribute("disabled"),
  ).toBe(true);
  await settle(() => {
    complete?.();
  });
  expect(useSmabar.getState().pluginsHidden).toEqual(["plugin:Clock:main"]);
  expect(card("Clock").querySelector("fieldset")?.disabled).toBe(false);
});
