// @vitest-environment happy-dom
import { act } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { fixtureCall } from "../../ipc/fixture";
import { resetFixtureStore } from "../../ipc/fixtureStore";
import type { InstalledPlugin, ThemeSummary } from "../../store/bar";
import { StorePage } from "./StorePage";
import {
  createThemeManagerTestHarness,
  flush,
  typeInput,
  type ThemeManagerTestHarness,
} from "./ThemeManager.testHarness";

// The browser fixture IS the contract's reference implementation, so the
// page is driven end to end through it; the spy only records the calls.
const { callMock } = vi.hoisted(() => ({
  callMock: vi.fn(
    (command: string, args?: Record<string, unknown>) =>
      new Promise<unknown>((resolve) => {
        resolve(fixtureCall(command, args));
      }),
  ),
}));
vi.mock("../../ipc/call", () => ({ call: callMock }));

let harness: ThemeManagerTestHarness;

beforeEach(() => {
  resetFixtureStore();
  callMock.mockClear();
  harness = createThemeManagerTestHarness();
});

afterEach(() => {
  harness.dispose();
});

/** Enough microtask turns for a command, its follow-ups and the re-render. */
async function settle(): Promise<void> {
  await act(async () => {
    for (let turn = 0; turn < 8; turn += 1) await Promise.resolve();
  });
}

function rows(): string[] {
  return [
    ...harness.container.querySelectorAll<HTMLElement>(
      ".settings-store-list .settings-store-name",
    ),
  ].map((name) => name.textContent);
}

/** The badge on each row; an available listing carries none. */
function statuses(): string[] {
  return [
    ...harness.container.querySelectorAll<HTMLElement>("li[data-store-entry]"),
  ].map(
    (item) =>
      item.querySelector(".settings-store-heading .sb-badge")?.textContent ??
      "",
  );
}

function row(name: string): HTMLElement {
  const match = [
    ...harness.container.querySelectorAll<HTMLElement>("li[data-store-entry]"),
  ].find(
    (candidate) =>
      candidate.querySelector(".settings-store-name")?.textContent === name,
  );
  if (match === undefined) throw new Error(`no row "${name}"`);
  return match;
}

function rowAction(name: string, action: string): HTMLButtonElement {
  const button = row(name).querySelector<HTMLButtonElement>(
    `[data-store-action="${action}"]`,
  );
  if (button === null) throw new Error(`no ${action} button on "${name}"`);
  return button;
}

function detailsButton(name: string): HTMLButtonElement {
  const button = row(name).querySelector<HTMLButtonElement>(
    "[data-store-details]",
  );
  if (button === null) throw new Error(`no Details button on "${name}"`);
  return button;
}

function detail(): HTMLElement {
  const region = harness.container.querySelector<HTMLElement>(
    ".settings-store-detail",
  );
  if (region === null) throw new Error("no open detail page");
  return region;
}

function detailAction(action: string): HTMLButtonElement {
  const button = detail().querySelector<HTMLButtonElement>(
    `[data-store-action="${action}"]`,
  );
  if (button === null) throw new Error(`no ${action} button on the page`);
  return button;
}

function question(): string {
  return (
    harness.container.querySelector("[data-confirm-row]")?.textContent ?? ""
  );
}

async function open(name: string): Promise<void> {
  await flush(() => {
    detailsButton(name).click();
  });
  await settle();
}

async function back(): Promise<void> {
  await flush(() => {
    harness.button("Back to the list").click();
  });
  await settle();
}

async function renderPlugins(): Promise<void> {
  await harness.render(<StorePage kind="plugin" />);
  await settle();
}

test("lists the catalog's plugins only, decisions first, then by name", async () => {
  await renderPlugins();
  expect(rows()).toEqual([
    "GitHub notifications",
    "Pomodoro",
    "Coin flipper",
    "Docker status",
    "Spotify lyrics",
  ]);
  expect(harness.container.querySelector('[role="status"]')?.textContent).toBe(
    "5 of 5 entries shown",
  );
  expect(statuses()).toEqual([
    "Update 1.3.0",
    "Modified locally",
    "Blocked",
    "",
    "Not compatible",
  ]);
  expect(harness.container.textContent).toContain(
    "it does not review the code",
  );
  // The buttons sit on the row: nothing to open before installing.
  expect(rowAction("Docker status", "install").textContent).toBe(
    "Install 0.6.2",
  );
  expect(row("Coin flipper").querySelector("[data-store-action]")).toBeNull();
  expect(row("Spotify lyrics").querySelector("[data-store-action]")).toBeNull();
});

test("the order is a choice: recommended, name, most stars or newest", async () => {
  await renderPlugins();
  const select = harness.container.querySelector<HTMLSelectElement>(
    'select[aria-label="Sort by"]',
  );
  if (select === null) throw new Error("no sort control");
  const choose = async (value: string) => {
    await flush(() => {
      select.value = value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
  };
  expect(select.value).toBe("state");
  await choose("name");
  expect(rows()).toEqual([
    "Coin flipper",
    "Docker status",
    "GitHub notifications",
    "Pomodoro",
    "Spotify lyrics",
  ]);
  // Dates open newest first.
  await choose("updatedAt");
  expect(rows()[0]).toBe("Spotify lyrics");
  expect(rows()[4]).toBe("Coin flipper");
  await choose("stars");
  expect(rows()).toEqual([
    "Spotify lyrics",
    "GitHub notifications",
    "Pomodoro",
    "Docker status",
    "Coin flipper",
  ]);
});

test("search narrows with every term and says so", async () => {
  await renderPlugins();
  const search = harness.input("Search the catalog");
  await flush(() => {
    typeInput(search, "docker cli");
  });
  expect(rows()).toEqual(["Docker status"]);
  expect(harness.container.querySelector('[role="status"]')?.textContent).toBe(
    "1 of 5 entries shown",
  );
  await flush(() => {
    typeInput(search, "docker spotify");
  });
  expect(rows()).toEqual([]);
  expect(harness.container.textContent).toContain(
    "Nothing matches your search.",
  );
});

test("Details opens a page with the exact source and the rendered readme", async () => {
  await renderPlugins();
  await open("Docker status");
  const region = detail();
  expect(region.getAttribute("role")).toBe("region");
  expect(region.getAttribute("aria-label")).toBe("Docker status");
  expect(harness.container.querySelector(".settings-store-list")).toBeNull();

  // The label is short; the URL is the title and the accessible name.
  const links = [...region.querySelectorAll(".settings-store-link")].map(
    (link) => link.getAttribute("title"),
  );
  expect(links).toContain("https://github.com/containerkat/smabar-plugins");
  expect(links).toContain(
    "https://github.com/containerkat/smabar-plugins/tree/c4d6e8f0a2b4c6d8e0f2a4b6c8d0e2f4a6b8c0d2/plugins/docker-status",
  );
  expect(links).toContain(
    "https://github.com/containerkat/smabar-plugins/releases/tag/docker-status-v0.6.2",
  );
  expect(region.textContent).toContain("Executable (exec)");
  expect(region.textContent).toContain("Needs external programs.");
  expect(region.textContent).toContain(
    "docker — smabar never installs these for you.",
  );
  const trust = [...region.querySelectorAll(".settings-store-trust li")].map(
    (fact) => fact.textContent,
  );
  expect(trust).toEqual([
    "Public source",
    "Catalog signature valid",
    "License declared: GPL-3.0-only",
  ]);
  expect(region.textContent).not.toMatch(/verified|safe|reviewed by/i);
  // The readme is the core's HTML, rendered as such.
  const readme = region.querySelector(".settings-readme");
  expect(readme?.querySelector("h1")?.textContent).toBe("Docker status");
  expect(readme?.textContent).toContain("Needs the docker CLI on PATH");
  expect(detailAction("install").textContent).toBe("Install 0.6.2");

  // The way back is focused; going back lands on the row's Details button.
  expect(document.activeElement).toBe(harness.button("Back to the list"));
  await back();
  expect(rows()).toHaveLength(5);
  await flush(() => {
    vi.runOnlyPendingTimers();
  });
  expect(document.activeElement).toBe(detailsButton("Docker status"));
});

test("readme links and images: links open through the core, images come from GitHub", async () => {
  await renderPlugins();
  await open("GitHub notifications");
  const readme = detail().querySelector<HTMLElement>(".settings-readme");
  if (readme === null) throw new Error("no readme");
  expect(readme.querySelector("table")).not.toBeNull();
  expect(readme.querySelector("img")?.getAttribute("src")).toMatch(
    /^https:\/\/raw\.githubusercontent\.com\//,
  );
  const link = readme.querySelector<HTMLAnchorElement>("a[href]");
  if (link === null) throw new Error("no readme link");
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  await flush(() => {
    link.dispatchEvent(click);
  });
  expect(click.defaultPrevented).toBe(true);
  expect(callMock).toHaveBeenCalledWith("open_url", {
    url: "https://github.com/mira-dev/smabar-github-notifications/blob/9f1c2e7a4b6d8c0e2f4a6b8c0d2e4f6a8b0c2d4e/CHANGELOG.md",
  });
});

test("facts links open through the core, never as navigation", async () => {
  await renderPlugins();
  await open("Docker status");
  const link = detail().querySelector<HTMLButtonElement>(
    '[aria-label="Open https://github.com/containerkat/smabar-plugins in the browser"]',
  );
  if (link === null) throw new Error("no repository link");
  await flush(() => {
    link.click();
  });
  expect(callMock).toHaveBeenCalledWith("open_url", {
    url: "https://github.com/containerkat/smabar-plugins",
  });
});

test("an incompatible or blocked listing explains itself and offers nothing", async () => {
  await renderPlugins();
  await open("Spotify lyrics");
  expect(detail().textContent).toContain(
    "Needs smabar 0.9.0 or newer; this is 0.2.0.",
  );
  expect(detail().querySelector("[data-store-action]")).toBeNull();
  await back();
  await open("Coin flipper");
  expect(detail().textContent).toContain(
    "Blocked by the store: sends clipboard contents to a third-party server.",
  );
  expect(detail().textContent).toContain("No license declared");
  expect(detail().textContent).toContain("The repository is archived");
  expect(detail().querySelector("[data-store-action]")).toBeNull();
});

test("installing from the row asks inline, names the source, then reports the install", async () => {
  await renderPlugins();
  await flush(() => {
    rowAction("Docker status", "install").click();
  });
  expect(callMock).not.toHaveBeenCalledWith(
    "store_install_plugin",
    expect.anything(),
  );
  expect(question()).toContain(
    "Install Docker status 0.6.2 from containerkat/smabar-plugins? smabar has not reviewed this code",
  );
  expect(document.activeElement).toBe(harness.button("Cancel"));
  await flush(() => {
    harness.button("Cancel").click();
  });
  expect(document.activeElement).toBe(rowAction("Docker status", "install"));

  await flush(() => {
    rowAction("Docker status", "install").click();
  });
  await flush(() => {
    harness.confirmAction().click();
  });
  await settle();
  expect(callMock).toHaveBeenCalledWith("store_install_plugin", {
    id: "docker-status",
    expectedVersion: "0.6.2",
    confirmModified: false,
  });
  // Installed now, so it moves ahead of the plain listings.
  expect(rows()).toEqual([
    "GitHub notifications",
    "Pomodoro",
    "Docker status",
    "Coin flipper",
    "Spotify lyrics",
  ]);
  expect(row("Docker status").querySelector(".sb-badge")?.textContent).toBe(
    "Installed",
  );
  expect(rowAction("Docker status", "uninstall").textContent).toBe("Uninstall");
  // The install button is gone; Details takes the focus instead.
  await flush(() => {
    vi.runOnlyPendingTimers();
  });
  expect(document.activeElement).toBe(detailsButton("Docker status"));

  await open("Docker status");
  expect(detail().textContent).toContain("Installed: 0.6.2 (c4d6e8f)");
  expect(detail().textContent).toContain("Content hash matches");

  const installed = fixtureCall("list_plugins") as InstalledPlugin[];
  expect(installed.find((plugin) => plugin.id === "docker-status")).toEqual(
    expect.objectContaining({
      origin: "community",
      version: "0.6.2",
      update: null,
    }),
  );
});

test("an update names both versions and a modified install warns first", async () => {
  await renderPlugins();
  await open("GitHub notifications");
  expect(detail().textContent).toContain("1.3.0 is listed");
  await flush(() => {
    detailAction("update").click();
  });
  expect(question()).toContain(
    "Update GitHub notifications from 1.2.0 to 1.3.0 from mira-dev/smabar-github-notifications?",
  );
  await flush(() => {
    harness.confirmAction().click();
  });
  await settle();
  expect(callMock).toHaveBeenCalledWith("store_install_plugin", {
    id: "github-notifications",
    expectedVersion: "1.3.0",
    confirmModified: false,
  });
  expect(detail().textContent).toContain("Installed: 1.3.0 (9f1c2e7)");

  await back();
  await open("Pomodoro");
  expect(detail().textContent).toContain(
    "The installed files differ from what the store installed.",
  );
  expect(detail().querySelector('[data-store-action="update"]')).toBeNull();
  expect(detailAction("uninstall")).not.toBeNull();
});

test("uninstalling goes through the plugin's own removal and re-reads the store", async () => {
  await renderPlugins();
  await flush(() => {
    rowAction("Pomodoro", "uninstall").click();
  });
  expect(question()).toContain(
    "Uninstall Pomodoro? Its data and settings are removed too.",
  );
  await flush(() => {
    harness.confirmAction().click();
  });
  await settle();
  expect(callMock).toHaveBeenCalledWith("remove_plugin", {
    pluginId: "pomodoro",
  });
  expect(rowAction("Pomodoro", "install").textContent).toBe("Install 2.0.1");
  expect(rows().indexOf("Pomodoro")).toBeGreaterThan(
    rows().indexOf("Docker status"),
  );
});

test("a refused install stays visible and returns focus to the button", async () => {
  await renderPlugins();
  await flush(() => {
    rowAction("Docker status", "install").click();
  });
  // Someone republished meanwhile: the fixture refuses a stale version.
  callMock.mockImplementationOnce(() =>
    Promise.reject(new Error('"docker-status" is listed as 0.7.0, not 0.6.2')),
  );
  await flush(() => {
    harness.confirmAction().click();
  });
  await settle();
  expect(
    harness.container.querySelector('[role="alert"]')?.textContent,
  ).toContain("listed as 0.7.0");
  expect(rowAction("Docker status", "install").disabled).toBe(false);
  await flush(() => {
    vi.runOnlyPendingTimers();
  });
  expect(document.activeElement).toBe(rowAction("Docker status", "install"));
});

test("the themes page lists themes only and installs a drop-in", async () => {
  await harness.render(<StorePage kind="theme" />);
  await settle();
  expect(rows()).toEqual(["Nord"]);
  await open("Nord");
  expect(detail().textContent).not.toContain("Runtime");
  expect(detail().textContent).toContain("Any");
  await back();
  await flush(() => {
    rowAction("Nord", "install").click();
  });
  expect(question()).toContain(
    "Install Nord 1.0.2 from frostpalette/smabar-themes?",
  );
  await flush(() => {
    harness.confirmAction().click();
  });
  await settle();
  expect(callMock).toHaveBeenCalledWith("store_install_theme", {
    name: "nord",
    expectedVersion: "1.0.2",
  });
  const themes = fixtureCall("list_themes") as ThemeSummary[];
  expect(themes.find((theme) => theme.name === "nord")?.source).toBe("dropin");

  await flush(() => {
    rowAction("Nord", "uninstall").click();
  });
  expect(question()).toContain("Delete the theme Nord?");
  await flush(() => {
    harness.confirmAction().click();
  });
  await settle();
  expect(callMock).toHaveBeenCalledWith("delete_theme", { name: "nord" });
  expect(
    (fixtureCall("list_themes") as ThemeSummary[]).some(
      (theme) => theme.name === "nord",
    ),
  ).toBe(false);
  expect(rowAction("Nord", "install").textContent).toBe("Install 1.0.2");
});
