import { beforeEach, expect, test } from "vitest";

import type { InstalledPlugin, ThemeSummary } from "../store/bar";
import { useSmabar } from "../store/bar";
import { fixtureCall } from "./fixture";
import { resetFixtureStore } from "./fixtureStore";
import type { LegalStatus } from "./legal";
import type { StoreEntry, StoreOverview } from "./store";
import { asEntries } from "./fixtureValues";
import { SYSTEM_DEFAULTS } from "../components/settings/defaults";

beforeEach(() => {
  useSmabar.setState(useSmabar.getInitialState(), true);
  resetFixtureStore();
});

function overview(): StoreOverview {
  return fixtureCall("store_overview") as StoreOverview;
}

test("System fixtures support autostart, registered plugins and every reset write", () => {
  expect(fixtureCall("set_autostart", { enabled: false })).toEqual({
    state: "ready",
    registered: false,
  });
  expect(fixtureCall("get_autostart_status")).toEqual({
    state: "ready",
    registered: false,
  });
  for (const write of SYSTEM_DEFAULTS) fixtureCall("update_config", write);
  expect(fixtureCall("set_autostart", { enabled: true })).toEqual({
    state: "ready",
    registered: true,
  });
  expect(fixtureCall("get_system_settings")).toMatchObject({
    mcp: { enabled: true, port: 7627 },
    rendering: { mode: "auto" },
  });
  expect(fixtureCall("get_plugins")).toEqual([]);
  expect(() => fixtureCall("set_autostart", { enabled: "false" })).toThrow();
});

test("the legal fixture is an accepted profile with all three documents", () => {
  const status = fixtureCall("legal_status") as LegalStatus;
  expect(status.required).toBe(false);
  expect(status.acceptedAt).not.toBeNull();
  expect(status.terms.updated).toBe(status.termsVersion);
  expect(
    status.license.html.startsWith("<h1>PolyForm Shield License 1.0.0</h1>"),
  ).toBe(true);
  for (const doc of [status.terms, status.privacy, status.license]) {
    expect(doc.html).not.toBe("");
  }
  expect(fixtureCall("legal_accept")).toEqual(status);
  expect(fixtureCall("legal_decline")).toBeNull();
});

function entry(id: string): StoreEntry {
  const found = overview().entries.find((candidate) => candidate.id === id);
  if (found === undefined) throw new Error(`no fixture entry ${id}`);
  return found;
}

test("raw shortcut coercion preserves every source", () => {
  const entries = asEntries([
    { id: "desktop", desktopId: "files.desktop" },
    { id: "path", path: String.raw`C:\Tools\tool.exe` },
    { id: "url", url: "https://example.com" },
    { id: "special", special: "computer" },
    { id: "separator", separator: true },
  ]);

  expect(entries[0]?.desktopId).toBe("files.desktop");
  expect(entries[1]?.path).toBe(String.raw`C:\Tools\tool.exe`);
  expect(entries[2]?.url).toBe("https://example.com");
  expect(entries[3]?.special).toBe("computer");
  expect(entries[4]?.separator).toBe(true);
});

test("raw shortcut coercion rejects an unknown special source", () => {
  expect(() => asEntries([{ id: "bad", special: "home" }])).toThrow(
    'fixture: "entry.special" must be one of computer, trash',
  );
});

test("the browser fixture pins a localized special item", () => {
  fixtureCall("pin_special_shortcut", { special: "trash" });

  let shortcuts = useSmabar.getState().shortcuts;
  expect(shortcuts.entries).toHaveLength(1);
  expect(shortcuts.entries[0]?.special).toBe("trash");
  expect(shortcuts.pinned[0]?.label).toBe("Trash");

  fixtureCall("update_config", {
    path: "shortcuts.pinned",
    value: shortcuts.entries,
  });
  shortcuts = useSmabar.getState().shortcuts;
  expect(shortcuts.entries[0]?.special).toBe("trash");
  expect(shortcuts.pinned[0]?.label).toBe("Trash");
});

test("a store install shows up in list_plugins as a Community Plugin and leaves again", () => {
  expect(entry("docker-status").installed).toBeNull();
  const plugins = () => fixtureCall("list_plugins") as InstalledPlugin[];
  expect(plugins().some((plugin) => plugin.id === "docker-status")).toBe(false);

  const next = fixtureCall("store_install_plugin", {
    id: "docker-status",
    expectedVersion: "0.6.2",
    confirmModified: false,
  }) as StoreOverview;
  const installed = next.entries.find((found) => found.id === "docker-status");
  expect(installed?.installed).toEqual(
    expect.objectContaining({ version: "0.6.2", origin: "store" }),
  );
  expect(plugins().find((plugin) => plugin.id === "docker-status")).toEqual(
    expect.objectContaining({
      origin: "community",
      version: "0.6.2",
      update: null,
      modified: false,
      blocked: null,
    }),
  );
  // The seeded update carries over into the Installed list's facts.
  expect(
    plugins().find((plugin) => plugin.id === "github-notifications")?.update,
  ).toBe("1.3.0");

  fixtureCall("remove_plugin", { pluginId: "docker-status" });
  expect(entry("docker-status").installed).toBeNull();
  expect(plugins().some((plugin) => plugin.id === "docker-status")).toBe(false);
});

test("the store fixture refuses what the core would refuse", () => {
  expect(() =>
    fixtureCall("store_install_plugin", {
      id: "docker-status",
      expectedVersion: "0.6.1",
      confirmModified: false,
    }),
  ).toThrow("listed as 0.6.2, not 0.6.1");
  expect(() =>
    fixtureCall("store_install_plugin", {
      id: "spotify-lyrics",
      expectedVersion: "2.1.0",
      confirmModified: false,
    }),
  ).toThrow("minSmabar");
  expect(() =>
    fixtureCall("store_install_plugin", {
      id: "pomodoro",
      expectedVersion: "2.0.1",
      confirmModified: false,
    }),
  ).toThrow("modified locally");
  expect(() =>
    fixtureCall("store_detail", { kind: "plugin", id: "nord" }),
  ).toThrow('no store listing "plugin:nord"');
  expect(() =>
    fixtureCall("store_install_theme", { name: "nord", expectedVersion: 1 }),
  ).toThrow('"expectedVersion" must be a string');
});

test("an installed theme is a drop-in until delete_theme takes it away", () => {
  const themes = () => fixtureCall("list_themes") as ThemeSummary[];
  expect(themes().some((theme) => theme.name === "nord")).toBe(false);
  fixtureCall("store_install_theme", {
    name: "nord",
    expectedVersion: "1.0.2",
  });
  expect(themes().find((theme) => theme.name === "nord")?.source).toBe(
    "dropin",
  );
  expect(entry("nord").installed?.version).toBe("1.0.2");
  fixtureCall("delete_theme", { name: "nord" });
  expect(themes().some((theme) => theme.name === "nord")).toBe(false);
  expect(entry("nord").installed).toBeNull();
});
