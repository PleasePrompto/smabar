import { getTiles, unregisterPluginTiles } from "../components/registry";
import { DEMO_PLUGIN } from "./fixturePluginTiles";
import {
  useSmabar,
  type InstalledPlugin,
  type SpecialShortcut,
} from "../store/bar";
import {
  fixtureSpecialLabel,
  fixtureAudio,
  fixtureSystem,
  updateConfig,
} from "./fixtureConfig";
import { ensureFixtureFont, listFixtureFonts } from "./fixtureFonts";
import {
  fixtureInstallPlugin,
  fixtureInstallTheme,
  fixtureStoreDetail,
  fixtureStoreThemePreview,
  fixtureStoreOverview,
  fixtureStorePlugins,
  fixtureStoreRefresh,
  fixtureUninstall,
} from "./fixtureStore";
import {
  deleteFixtureTheme,
  exportFixtureTheme,
  fixtureExportDir,
  importFixtureTheme,
  listThemes,
  saveCustomTheme,
} from "./fixtureThemes";
import {
  asBoolean,
  asChoice,
  asString,
  SPECIAL_SHORTCUTS,
  STORE_KINDS,
} from "./fixtureValues";
import type { LegalStatus } from "./legal";

/**
 * Browser-dev stand-ins for the Tauri commands the settings panel uses
 * (dispatched by ipc/call.ts outside the Tauri window). Reads serve sample
 * data, writes mutate the store directly — the panel behaves like the real
 * thing without a core. Never bundled into the invoke path (lazy import).
 */

interface FixtureApp {
  desktopId: string;
  name: string;
  comment?: string;
}

// Browser fixture only: never writes a real login registration.
let autostartRegistered = true;

// An accepted profile: browser dev has no `get_ui_state`, so the store's
// default keeps the gate closed and the legal group only shows the texts.
const FIXTURE_LEGAL: LegalStatus = {
  required: false,
  termsVersion: "2026-09-07",
  privacyVersion: "2026-09-07",
  acceptedAt: Date.UTC(2026, 8, 7, 9, 30),
  terms: {
    title: "Terms of use",
    updated: "2026-09-07",
    html: "<h2>1. Provider and scope</h2><p>Sample terms of use from the browser fixture.</p>",
  },
  privacy: {
    title: "Privacy notice for the desktop app",
    updated: "2026-09-07",
    html: "<h2>1. Controller</h2><p>Sample privacy notice from the browser fixture.</p>",
  },
  license: {
    title: "PolyForm Shield 1.0.0",
    updated: null,
    html: "<h1>PolyForm Shield License 1.0.0</h1><p>Sample license text from the browser fixture.</p>",
  },
};

const APPS: FixtureApp[] = [
  "Files",
  "Browser",
  "Terminal",
  "Editor",
  "Music",
  "Mail",
  "Photos",
  "Chat",
  "Calendar",
  "Notes",
  "Camera",
  "Maps",
  "Games",
  "Meet",
].map((name) => ({
  desktopId: `${name.toLowerCase()}.desktop`,
  name,
  comment: `Sample ${name} application`,
}));

/** Seeds sample pins once so the dock zone has content (DevFixture). */
export function seedShortcuts(): void {
  const store = useSmabar.getState();
  if (store.shortcuts.pinned.length > 0) return;
  const sample = APPS.slice(0, 8);
  store.setShortcuts({
    ...store.shortcuts,
    pinned: sample.map((app, i) => ({
      id: `dev-${String(i)}`,
      label: app.name,
      icons: [],
      desktopId: app.desktopId,
      separator: false,
    })),
    entries: sample.map((app, i) => ({
      id: `dev-${String(i)}`,
      desktopId: app.desktopId,
    })),
  });
}

/** Serves one command from fixture data / store mutations. */
export function fixtureCall(
  command: string,
  args?: Record<string, unknown>,
): unknown {
  switch (command) {
    // Plain-browser dev has no core to log to; the devtools console already
    // has the message already.
    case "ui_log":
      return null;
    case "choose_settings_file":
      return null; // Browser fixtures have no native filesystem picker.
    case "list_apps":
      return listApps(args?.query);
    case "get_app_icon":
      return null;
    case "list_themes":
      return listThemes();
    case "save_custom_theme": {
      const name = asString(args?.name, "name");
      saveCustomTheme(name, args?.overwrite === true);
      updateConfig("theme", name);
      return listThemes();
    }
    case "delete_theme": {
      const name = asString(args?.name, "name");
      // Like the real command: switch away FIRST when deleting the active
      // theme, so the store never points at a missing document.
      if (useSmabar.getState().theme === name) updateConfig("theme", "default");
      deleteFixtureTheme(name);
      fixtureUninstall("theme", name);
      return listThemes();
    }
    case "export_theme":
      return exportFixtureTheme(
        asString(args?.name, "name"),
        typeof args?.directory === "string" ? args.directory : "",
      );
    case "import_theme":
      importFixtureTheme(
        asString(args?.path, "path"),
        args?.overwrite === true,
      );
      return listThemes();
    case "get_theme_export_dir":
      return fixtureExportDir();
    case "font_list":
      return listFixtureFonts(
        args?.query,
        args?.source,
        args?.monospaced,
        args?.limit,
      );
    case "ensure_google_font":
      return ensureFixtureFont(args?.id);
    case "get_runtime_status":
      return { state: "ready" };
    case "retry_provisioning":
      return null;
    // Browser dev only reaches this through "Check now": serve a release so
    // the available state can be designed without a store.
    case "check_update":
      return {
        version: "0.2.0",
        notes: "Sample release notes from the browser fixture.",
        date: "2026-08-29T08:00:00Z",
        installer: "system",
      };
    case "install_update":
      return { path: "/tmp/smabar_0.2.0_amd64.deb", opened: true };
    case "get_system_settings":
      return fixtureSystem;
    case "get_autostart_status":
      return { state: "ready", registered: autostartRegistered };
    case "get_reservation_status":
    case "request_reservation_access":
      return "ready";
    case "set_autostart":
      autostartRegistered = asBoolean(args?.enabled, "enabled");
      return { state: "ready", registered: autostartRegistered };
    case "get_audio_settings":
      return structuredClone(fixtureAudio);
    case "legal_status":
    case "legal_accept":
      return FIXTURE_LEGAL;
    case "legal_decline":
      return null;
    case "pin_shortcut":
      if (args?.separator === true) {
        pinSeparator();
      } else if (args?.url !== undefined) {
        pinWebsite(asString(args.url, "url"));
      } else {
        pinShortcut(asString(args?.desktopId, "desktopId"));
      }
      return null;
    case "pin_special_shortcut":
      pinSpecial(asChoice(args?.special, SPECIAL_SHORTCUTS, "special"));
      return null;
    case "unpin_shortcut":
      unpinShortcut(asString(args?.id, "id"));
      return null;
    case "update_config":
      updateConfig(asString(args?.path, "path"), args?.value);
      return null;
    case "list_plugins":
      return listPlugins();
    case "get_plugins":
      return Object.entries(useSmabar.getState().pluginSchemas).map(
        ([pluginId, info]) => ({
          pluginId,
          ...info,
          tiles: getTiles().flatMap((entry) =>
            entry.pluginId === pluginId ? [entry.tile] : [],
          ),
        }),
      );
    case "remove_plugin": {
      const pluginId = asString(args?.pluginId, "pluginId");
      removeFixturePlugin(pluginId);
      fixtureUninstall("plugin", pluginId);
      return null;
    }
    case "store_overview":
      return fixtureStoreOverview();
    case "store_refresh":
      return fixtureStoreRefresh();
    case "store_detail":
      return fixtureStoreDetail(
        asChoice(args?.kind, STORE_KINDS, "kind"),
        asString(args?.id, "id"),
      );
    case "store_theme_preview":
      return fixtureStoreThemePreview(
        asString(args?.name, "name"),
        asString(args?.expectedCommit, "expectedCommit"),
      );
    case "store_install_plugin":
      return fixtureInstallPlugin(
        asString(args?.id, "id"),
        asString(args?.expectedVersion, "expectedVersion"),
        asBoolean(args?.confirmModified, "confirmModified"),
      );
    case "store_install_theme":
      return fixtureInstallTheme(
        asString(args?.name, "name"),
        asString(args?.expectedVersion, "expectedVersion"),
      );
    // Demo plugin tiles (fixturePluginTiles.ts) have no process behind them —
    // their actions and link opens are accepted and dropped. The browser
    // shell has no desktop entries to launch either.
    case "plugin_action":
    case "open_url":
    case "launch_shortcut":
      return null;
    default:
      throw new Error(`no browser-dev fixture for command "${command}"`);
  }
}

function listApps(query: unknown): FixtureApp[] {
  const needle = typeof query === "string" ? query.toLowerCase() : "";
  if (needle === "") return APPS;
  return APPS.filter(
    (app) =>
      app.name.toLowerCase().includes(needle) ||
      (app.comment?.toLowerCase().includes(needle) ?? false),
  );
}

function pinShortcut(desktopId: string): void {
  const app = APPS.find((entry) => entry.desktopId === desktopId);
  if (!app) throw new Error(`fixture: unknown desktopId "${desktopId}"`);
  const store = useSmabar.getState();
  const { pinned, entries } = store.shortcuts;
  const id = `dev-pin-${String(Date.now())}`;
  store.setShortcuts({
    ...store.shortcuts,
    pinned: [
      ...pinned,
      { id, label: app.name, icons: [], desktopId, separator: false },
    ],
    entries: [...entries, { id, desktopId }],
  });
}

/** Mirrors the core's website pin: host label plus the site's icon URLs. */
function pinWebsite(url: string): void {
  // Throws on a malformed URL, like the core's validation does.
  const { host } = new URL(url);
  const store = useSmabar.getState();
  const { pinned, entries } = store.shortcuts;
  const id = `dev-web-${String(Date.now())}`;
  store.setShortcuts({
    ...store.shortcuts,
    pinned: [
      ...pinned,
      {
        id,
        label: host.replace(/^www\./, ""),
        icons: [
          `https://${host}/apple-touch-icon.png`,
          `https://${host}/favicon.ico`,
        ],
        separator: false,
      },
    ],
    entries: [...entries, { id, url }],
  });
}

function pinSpecial(special: SpecialShortcut): void {
  const store = useSmabar.getState();
  const { pinned, entries } = store.shortcuts;
  const id = `dev-special-${special}-${String(Date.now())}`;
  store.setShortcuts({
    ...store.shortcuts,
    pinned: [
      ...pinned,
      {
        id,
        label: fixtureSpecialLabel(special),
        icons: [],
        separator: false,
      },
    ],
    entries: [...entries, { id, special }],
  });
}

function pinSeparator(): void {
  const store = useSmabar.getState();
  const { pinned, entries } = store.shortcuts;
  const id = `dev-separator-${String(Date.now())}`;
  store.setShortcuts({
    ...store.shortcuts,
    pinned: [...pinned, { id, label: "", icons: [], separator: true }],
    entries: [...entries, { id, separator: true }],
  });
}

function unpinShortcut(id: string): void {
  const store = useSmabar.getState();
  const { pinned, entries } = store.shortcuts;
  store.setShortcuts({
    ...store.shortcuts,
    pinned: pinned.filter((pin) => pin.id !== id),
    entries: entries.filter((entry) => entry.id !== id),
  });
}

/**
 * The installed plugins the real `list_plugins` command reports. Browser-dev
 * has no supervisor, so the demo plugin's schema entry stands in for the
 * manifest and the deactivation list supplies the status; the store fixture
 * adds what the panel installed from the Community Store.
 */
function listPlugins(): InstalledPlugin[] {
  const store = useSmabar.getState();
  const demos = Object.entries(store.pluginSchemas).map(
    ([id, info]): InstalledPlugin => ({
      id,
      name: info.name,
      description: null,
      settingsSchema: info.settingsSchema ?? null,
      tiles: id === DEMO_PLUGIN.pluginId ? DEMO_PLUGIN.tiles : [],
      status: store.pluginsDeactivated.includes(id) ? "deactivated" : "running",
      origin: "base",
      version: "0.1.0",
      update: null,
      modified: false,
      blocked: null,
    }),
  );
  return [...demos, ...fixtureStorePlugins()];
}

/** Deleting in browser-dev drops the demo plugin from the shell's own state. */
function removeFixturePlugin(pluginId: string): void {
  const store = useSmabar.getState();
  const gone = unregisterPluginTiles(pluginId);
  store.dropPluginUi(pluginId, gone);
  store.setPluginSchema(pluginId, null);
  store.setPluginsDeactivated(
    store.pluginsDeactivated.filter((id) => id !== pluginId),
  );
  store.bumpRegistryVersion();
}
