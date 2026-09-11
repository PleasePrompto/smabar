// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import kitCss from "../../styles/ui-kit.css?raw";
import { useSmabar } from "../../store/bar";
import { SettingsPanel } from "./SettingsPanel";
import { SystemTab } from "./SystemTab";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../../ipc/call", () => ({ call: callMock }));
const { closeSurfaceMock } = vi.hoisted(() => ({
  closeSurfaceMock: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../ipc/surface", () => ({
  closeCurrentSurface: closeSurfaceMock,
}));

let container: HTMLDivElement;
let root: Root;
let opener: HTMLButtonElement;

beforeEach(() => {
  callMock.mockReturnValue(new Promise(() => undefined));
  useSmabar.setState(useSmabar.getInitialState(), true);
  container = document.createElement("div");
  opener = document.createElement("button");
  document.body.append(opener, container);
  opener.focus();
  root = createRoot(container);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  opener.remove();
  container.remove();
  callMock.mockReset();
  closeSurfaceMock.mockClear();
});

function blockTitles(): string[] {
  return [...document.querySelectorAll(".settings-block-title")].map(
    (heading) => heading.textContent,
  );
}

function navButton(label: string): HTMLButtonElement {
  const button = document.querySelector<HTMLButtonElement>(
    `.settings-nav-section > button[aria-label="${label}"]`,
  );
  if (button === null) throw new Error(`${label} navigation button is missing`);
  return button;
}

function subnavLabels(): string[] {
  return [...document.querySelectorAll(".settings-subnav button")].map(
    (button) => button.textContent,
  );
}

function subnavButton(label: string): HTMLButtonElement {
  const match = [
    ...document.querySelectorAll<HTMLButtonElement>(".settings-subnav button"),
  ].find((button) => button.textContent === label);
  if (match === undefined)
    throw new Error(`no sub-navigation entry "${label}"`);
  return match;
}

function pageEntry(label: string): HTMLButtonElement {
  const match = [
    ...document.querySelectorAll<HTMLButtonElement>(".settings-subnav-page"),
  ].find((button) => button.textContent === label);
  if (match === undefined) throw new Error(`no page entry "${label}"`);
  return match;
}

function navLabels(): (string | null)[] {
  return [...document.querySelectorAll(".settings-nav-section > button")].map(
    (button) => button.getAttribute("aria-label"),
  );
}

test("five groups, Design on its own, the store pages listed under Plugins and Design", () => {
  useSmabar.setState({ settingsGroup: "bar" });
  act(() => {
    root.render(<SettingsPanel />);
  });

  expect(navLabels()).toEqual([
    "Bar",
    "Design",
    "Shortcuts",
    "Plugins",
    "System",
  ]);
  expect(blockTitles()).toEqual([
    "Placement",
    "Arrangement",
    "Window behavior",
    "Size",
    "Notifications",
  ]);
  expect(document.querySelector(".settings-subnav-page")).toBeNull();

  act(() => {
    navButton("Design").click();
  });
  expect(blockTitles()).toEqual([
    "Theme",
    "Your themes",
    "Colors",
    "Typography",
    "Surfaces",
  ]);
  expect(pageEntry("Theme Store").getAttribute("aria-current")).toBeNull();

  act(() => {
    navButton("Plugins").click();
  });
  expect(blockTitles()).toEqual(["Installed", "Appearance", "Hover"]);

  // The page replaces the group's sections; the group stays highlighted.
  act(() => {
    pageEntry("Plugin Store").click();
  });
  expect(useSmabar.getState().settingsGroup).toBe("plugins/store");
  expect(document.querySelector(".settings-store-page")).not.toBeNull();
  expect(blockTitles()).toEqual([]);
  expect(navButton("Plugins").getAttribute("aria-current")).toBe("page");
  // The group's sections stay listed beside the page entry.
  expect(subnavLabels()).toEqual([
    "Installed",
    "Plugin Store",
    "Appearance",
    "Hover",
  ]);
  expect(pageEntry("Plugin Store").getAttribute("aria-current")).toBe("page");

  const back = document.querySelector<HTMLButtonElement>(
    '.settings-store-page [aria-label="Back to the list"]',
  );
  if (back === null) throw new Error("the store page has no way back");
  act(() => {
    back.click();
  });
  expect(useSmabar.getState().settingsGroup).toBe("plugins");
  expect(blockTitles()).toEqual(["Installed", "Appearance", "Hover"]);

  // A section entry clicked on the page leads back to that section.
  act(() => {
    pageEntry("Plugin Store").click();
  });
  const scrolled = vi.fn();
  Element.prototype.scrollIntoView = scrolled;
  act(() => {
    subnavButton("Hover").click();
  });
  expect(useSmabar.getState().settingsGroup).toBe("plugins");
  expect(scrolled).toHaveBeenCalledTimes(1);
  const target = document.querySelectorAll(".settings-block")[2];
  expect(scrolled.mock.instances[0]).toBe(target);
  // Lit up, so the click is seen to land even when nothing had to scroll.
  expect(target?.hasAttribute("data-highlight")).toBe(true);

  const previousContent =
    container.querySelector<HTMLElement>("#settings-content");
  if (previousContent === null) throw new Error("no settings content");
  previousContent.scrollTop = 400;
  act(() => {
    navButton("System").click();
  });
  expect(
    container.querySelector<HTMLElement>("#settings-content")?.scrollTop,
  ).toBe(0);
  // Rendering and application updates wait for get_system_settings; the
  // mock never answers here, so the channel remains unknown.
  expect(blockTitles()).toEqual([
    "About smabar",
    "General",
    "Agent access",
    "Python runtime",
    "smabar audio",
    "Legal",
  ]);
  expect(document.querySelector(".settings-info-logo")).not.toBeNull();
  expect(document.querySelector(".settings-info-version")?.textContent).toBe(
    "Version dev",
  );
});

test.each(["app", "store"])(
  "application update controls follow the %s channel",
  async (updateChannel) => {
    callMock.mockImplementation((command: string) =>
      command === "get_system_settings"
        ? Promise.resolve({
            updateChannel,
            languages: ["en"],
            mcp: { enabled: true, port: 7627 },
            rendering: null,
          })
        : new Promise(() => undefined),
    );
    await act(async () => {
      root.render(<SystemTab />);
      await Promise.resolve();
    });
    expect(blockTitles().includes("Updates")).toBe(updateChannel === "app");
  },
);

test("a page id opened from outside lands on its page with its group highlighted", () => {
  useSmabar.setState({ settingsGroup: "design/themes" });
  act(() => {
    root.render(<SettingsPanel />);
  });
  expect(navButton("Design").getAttribute("aria-current")).toBe("page");
  expect(
    document.querySelector(".settings-store-page")?.getAttribute("aria-label"),
  ).toBe("Theme Store");
});

test("unaccepted terms leave only the Legal group, whatever page was requested", () => {
  useSmabar.setState({ legalRequired: true, settingsGroup: "plugins/store" });
  act(() => {
    root.render(<SettingsPanel />);
  });

  expect(navLabels()).toEqual(["Legal"]);
  expect(navButton("Legal").getAttribute("aria-current")).toBe("page");
  expect(document.querySelector('section[aria-label="Legal"]')).not.toBeNull();
  expect(document.querySelector(".settings-store-page")).toBeNull();
  // The legal texts are still on their way; the mock never answers here.
  expect(document.querySelector(".settings-help")?.textContent).toBe(
    "Loading the legal texts…",
  );
  expect(callMock).toHaveBeenCalledWith("legal_status");

  // Accepting (the core says so through legal-changed) brings the rest back
  // and retires the Legal group from the navigation.
  act(() => {
    useSmabar.getState().setLegalRequired(false);
  });
  expect(navLabels()).toEqual([
    "Bar",
    "Design",
    "Shortcuts",
    "Plugins",
    "System",
  ]);
  expect(document.querySelector(".settings-store-page")).not.toBeNull();
});

test("after acceptance a request for the legal group lands on System's folded block", () => {
  useSmabar.setState({ legalRequired: false, settingsGroup: "legal" });
  act(() => {
    root.render(<SettingsPanel />);
  });

  expect(navButton("System").getAttribute("aria-current")).toBe("page");
  const block = document.querySelector<HTMLDetailsElement>(
    'details[aria-label="Legal"]',
  );
  expect(block).not.toBeNull();
  expect(block?.open).toBe(false);
  expect(callMock).toHaveBeenCalledWith("legal_status");
});

test("settings behaves as a named dialog and closes its native surface", () => {
  useSmabar.setState({ settingsGroup: "unknown" });
  act(() => {
    root.render(<SettingsPanel />);
  });

  const dialog = document.querySelector<HTMLElement>('[role="dialog"]');
  expect(dialog?.getAttribute("aria-labelledby")).toBe("settings-title");
  expect(
    document.querySelector('[aria-label="Bar"]')?.getAttribute("aria-current"),
  ).toBe("page");
  expect(document.activeElement?.getAttribute("aria-label")).toBe(
    "Close settings",
  );

  const close = document.querySelector<HTMLButtonElement>(
    '[aria-label="Close settings"]',
  );
  if (close === null) throw new Error("close button is missing");
  act(() => {
    close.click();
  });
  expect(closeSurfaceMock).toHaveBeenCalledOnce();
});

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
    root.render(<SystemTab />);
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

test("returning from the store waits for a plugin card before scrolling and opening it", async () => {
  const installed = [
    {
      id: "clock",
      name: "Clock",
      description: null,
      settingsSchema: null,
      tiles: [],
      status: "running",
      origin: "base",
      version: "1.0.0",
      update: null,
      modified: false,
      blocked: null,
    },
  ];
  let finish: ((plugins: typeof installed) => void) | undefined;
  let delayed = false;
  callMock.mockImplementation((command: string) =>
    command === "list_plugins"
      ? delayed
        ? new Promise((resolve) => {
            finish = resolve;
          })
        : Promise.resolve(installed)
      : new Promise(() => undefined),
  );
  useSmabar.setState({ settingsGroup: "plugins" });
  await act(async () => {
    root.render(<SettingsPanel />);
    await Promise.resolve();
  });
  expect(subnavLabels()).toContain("Clock");
  act(() => {
    pageEntry("Plugin Store").click();
  });
  delayed = true;
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  await act(async () => {
    subnavButton("Clock").click();
    await Promise.resolve();
  });
  expect(scroll).not.toHaveBeenCalled();
  await act(async () => {
    finish?.(installed);
    await Promise.resolve();
  });
  expect(scroll).toHaveBeenCalledOnce();
  expect(
    document.querySelector<HTMLDetailsElement>(".settings-plugin-card details")
      ?.open,
  ).toBe(true);
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
