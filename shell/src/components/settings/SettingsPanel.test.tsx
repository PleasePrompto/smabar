// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { useSmabar } from "../../store/bar";
import { setLocale } from "../../i18n/t";
import { fixtureCall } from "../../ipc/fixture";
import kitCss from "../../styles/ui-kit.css?raw";
import { SettingsPanel } from "./SettingsPanel";
import { SystemTab } from "./SystemTab";
import { pageDefaults } from "./pageDefaults";
import { resolveSettingsPage } from "./settingsPages";
const { callMock, closeSurfaceMock } = vi.hoisted(() => ({
  callMock: vi.fn(),
  closeSurfaceMock: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../ipc/call", () => ({ call: callMock }));
vi.mock("../../ipc/surface", () => ({ closeCurrentSurface: closeSurfaceMock }));
let container: HTMLDivElement;
let root: Root;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  setLocale({});
  useSmabar.setState(useSmabar.getInitialState(), true);
  callMock.mockImplementation(
    (command: string, args?: Record<string, unknown>) =>
      Promise.resolve(fixtureCall(command, args)),
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  callMock.mockReset();
  closeSurfaceMock.mockClear();
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: false });
});
async function render() {
  await act(async () => {
    root.render(<SettingsPanel />);
    await Promise.resolve();
  });
}
function click(selector: string) {
  const button = container.querySelector<HTMLButtonElement>(selector);
  if (button === null) throw new Error(selector);
  act(() => {
    button.click();
  });
}
function titles() {
  return [...container.querySelectorAll(".settings-block-title")].map(
    (node) => node.textContent,
  );
}

test("four stable groups separate global design from content management", async () => {
  await render();
  expect(
    [...container.querySelectorAll(".settings-nav-section > button")].map(
      (button) => button.getAttribute("aria-label"),
    ),
  ).toEqual(["Bar & Design", "Shortcuts", "Plugins", "System"]);
  expect(titles()).toEqual(["Placement", "Arrangement", "Size"]);
  expect(
    container.querySelector('.settings-subnav [aria-current="page"]')
      ?.textContent,
  ).toBe("Layout");
  click("#settings-tab-shortcuts");
  expect(titles()).toEqual(["Pinned", "Add"]);
  expect(container.textContent).not.toContain("Magnification");
});
test.each([
  ["bar", "bar/layout"],
  ["design", "bar/themes"],
  ["design/themes", "bar/community"],
  ["system/updates", "system/about"],
  ["legal", "system/legal"],
  ["unknown", "bar/layout"],
])("legacy entry %s resolves to %s", (from, to) => {
  expect(resolveSettingsPage(from)).toBe(to);
});
test("plugins have a labeled indented list and deactivated entries remain clickable", async () => {
  useSmabar.setState({
    settingsGroup: "plugins",
    pluginsDeactivated: ["clock"],
    pluginSchemas: { clock: { name: "Clock", settingsSchema: {} } },
  });
  await render();
  expect(container.querySelector(".settings-plugin-nav h3")?.textContent).toBe(
    "Installed",
  );
  const off = container.querySelector<HTMLButtonElement>(
    ".settings-plugin-nav [data-deactivated]",
  );
  expect(off).not.toBeNull();
  expect(off?.disabled).toBe(false);
  expect(off?.textContent).toBe("Clock");
  await act(async () => {
    off?.click();
    await Promise.resolve();
  });
  expect(container.querySelectorAll(".settings-plugin-card")).toHaveLength(1);
  expect(container.querySelector(".settings-plugin-details")).not.toBeNull();
  expect(container.querySelector("details.settings-plugin-details")).toBeNull();
});

test("the sidebar footer changes language and links its update chip to app updates", async () => {
  await render();
  const footer = container.querySelector(".settings-sidebar-footer");
  expect(
    footer?.querySelector('[role="img"]')?.getAttribute("aria-label"),
  ).toBe("smabar — the smart taskbar");
  const select = footer?.querySelector("select");
  expect(select).not.toBeNull();
  await act(async () => {
    if (select === null || select === undefined)
      throw new Error("language picker missing");
    select.value = "de";
    select.dispatchEvent(new Event("change", { bubbles: true }));
    await Promise.resolve();
  });
  expect(callMock).toHaveBeenCalledWith("update_config", {
    path: "language",
    value: "de",
  });
  act(() => {
    useSmabar.setState({
      updateChannel: "app",
      updateOffer: {
        version: "2.0.0",
        notes: null,
        date: null,
        installer: "system",
      },
    });
  });
  expect(
    footer?.querySelector(".settings-version-chip[data-update]"),
  ).not.toBeNull();
  await act(async () => {
    footer?.querySelector<HTMLButtonElement>(".settings-version-chip")?.click();
    await Promise.resolve();
  });
  expect(useSmabar.getState().settingsGroup).toBe("system/about");
  expect(
    container.querySelector('[aria-label="About & Updates"]'),
  ).not.toBeNull();
});
test("legal gate blocks every normal page and acceptance restores navigation", async () => {
  useSmabar.setState({ settingsGroup: "plugins/store", legalRequired: true });
  await render();
  expect(
    container.querySelectorAll(".settings-nav-section > button"),
  ).toHaveLength(1);
  expect(
    container.querySelector(".settings-nav-section > button")?.textContent,
  ).toBe("Legal");
  expect(container.querySelector(".settings-store-page")).toBeNull();
  await act(async () => {
    useSmabar.setState({ legalRequired: false, settingsGroup: "legal" });
    await Promise.resolve();
  });
  expect(
    container
      .querySelector("#settings-tab-system")
      ?.getAttribute("aria-current"),
  ).toBe("page");
  expect(
    container.querySelector('.settings-subnav [aria-current="page"]')
      ?.textContent,
  ).toBe("Legal");
});
test("settings is a named dialog and Escape and close use its native surface", async () => {
  await render();
  expect(
    container.querySelector('[role="dialog"]')?.getAttribute("aria-labelledby"),
  ).toBe("settings-title");
  click('[aria-label="Close settings"]');
  expect(closeSurfaceMock).toHaveBeenCalledOnce();
  act(() => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  });
  expect(closeSurfaceMock).toHaveBeenCalledTimes(2);
});
test("appearance and behavior resets preserve pins, plugin status and other pages", () => {
  const appearance = pageDefaults("appearance").map(({ path }) => path);
  expect(appearance).toContain("appearance.tileChrome");
  expect(appearance).toContain("shortcuts.labels");
  expect(appearance).not.toContain("shortcuts.pinned");
  expect(appearance).not.toContain("pluginsDeactivated");
  expect(pageDefaults("layout").map(({ path }) => path)).not.toContain(
    "layout.behavior",
  );
  expect(pageDefaults("behavior").map(({ path }) => path)).toContain(
    "effects.hoverPeek.delayMs",
  );
});
test.each(["app", "store"])(
  "updates respect the %s distribution channel",
  async (updateChannel) => {
    callMock.mockImplementation((command: string) =>
      command === "get_system_settings"
        ? Promise.resolve({
            updateChannel,
            languages: ["en"],
            mcp: { enabled: true, port: 7627 },
            rendering: null,
          })
        : Promise.resolve(fixtureCall(command)),
    );
    await act(async () => {
      root.render(<SystemTab page="about" />);
      await Promise.resolve();
    });
    expect(titles().includes("Updates")).toBe(updateChannel === "app");
  },
);
test("Linux rendering offers automatic, native, and software modes", async () => {
  callMock.mockImplementation((command: string) => {
    if (command === "get_system_settings") {
      return Promise.resolve({
        languages: ["en"],
        mcp: { enabled: true, port: 7627 },
        rendering: {
          mode: "auto",
          startupMode: "auto",
          applied: "pinned",
        },
      });
    }
    if (command === "get_audio_settings")
      return Promise.resolve({
        volume: 100,
        muted: false,
        notificationSounds: true,
        plugins: {},
      });
    if (command === "get_plugins") return Promise.resolve([]);
    if (command === "get_autostart_status")
      return Promise.resolve({ state: "unavailable" });
    // The folded legal block waits for its texts; not this test's concern.
    if (command === "legal_status") return new Promise(() => undefined);
    return Promise.resolve(undefined);
  });

  act(() => {
    root.render(<SystemTab page="advanced" />);
  });
  await act(() => Promise.resolve());

  expect(
    [...container.querySelectorAll<HTMLButtonElement>(".sb-choice")]
      .map((button) => button.textContent)
      .filter((label) =>
        ["Automatic", "Native graphics", "Software mode"].includes(label),
      ),
  ).toEqual(["Automatic", "Native graphics", "Software mode"]);

  // The only automated guard for the pinned i18n key: t() is untyped, so a
  // missing key would otherwise render as its raw name without failing tsc.
  expect(container.querySelector('[role="status"]')?.textContent).toBe(
    "This start renders on the non-NVIDIA GPU — the NVIDIA driver would flash stale frames.",
  );

  const software = [
    ...container.querySelectorAll<HTMLButtonElement>(".sb-choice"),
  ].find((button) => button.textContent === "Software mode");
  if (software === undefined)
    throw new Error("Software mode choice is missing");
  act(() => {
    software.click();
  });

  expect(software.getAttribute("aria-pressed")).toBe("true");
  expect(container.querySelector(".sb-warn")?.textContent).toBe(
    "Restart smabar to apply this change.",
  );
  expect(callMock).toHaveBeenCalledWith("update_config", {
    path: "rendering",
    value: "software",
  });
});

test("native resize grips stay at the edges without kit content spacing", () => {
  const internals = Object.getOwnPropertyDescriptor(
    window,
    "__TAURI_INTERNALS__",
  );
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  useSmabar.setState({ legalRequired: true });
  const style = document.createElement("style");
  style.textContent = kitCss;
  document.head.append(style);
  try {
    act(() => {
      root.render(<SettingsPanel />);
    });
    const handles = container.querySelectorAll(
      ".settings-resize-edge, .settings-resize-control",
    );
    expect(handles).toHaveLength(9);
    for (const handle of handles) {
      expect(getComputedStyle(handle).marginBlockStart).toBe("0");
    }
  } finally {
    style.remove();
    if (internals)
      Object.defineProperty(window, "__TAURI_INTERNALS__", internals);
    else Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  }
});
