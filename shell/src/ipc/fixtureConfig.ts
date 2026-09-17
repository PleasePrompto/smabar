import {
  useSmabar,
  type ResolvedShortcut,
  type ShortcutConfigEntry,
  type SpecialShortcut,
} from "../store/bar";
import { t } from "../i18n/t";
import { applyTheme, applyTokenOverrides } from "../theme/apply";
import { DEFAULT_THEME, getFixtureTheme } from "./fixtureThemes";
import {
  asBoolean,
  asChoice,
  asEntries,
  asNumber,
  asString,
  asStrings,
  asTokenMap,
} from "./fixtureValues";
import { setDemoPluginRegistered } from "./fixturePluginTiles";
import type { RenderingStatus } from "../components/settings/RenderingGroup";

/**
 * Browser-dev stand-in for `update_config`: every settable path the settings
 * panel can write, mapped onto the store the way the core would echo it back.
 * Lives in its own module so the command fixture stays focused — the same
 * reason fixtureThemes.ts is separate.
 */

/**
 * What `get_system_settings` answers here: the locales the core would find in
 * `~/.smabar/locales/` (the two bundled ones) and the MCP config, which has
 * no home in the shell store — only the System panel renders from it.
 */
export const fixtureSystem: {
  updateChannel: "app" | "store";
  languages: string[];
  mcp: { enabled: boolean; port: number };
  rendering: RenderingStatus;
} = {
  updateChannel: "app",
  languages: ["de", "en"],
  mcp: { enabled: true, port: 7627 },
  rendering: { mode: "auto", startupMode: "auto", applied: "native" },
};

export const fixtureAudio: {
  volume: number;
  muted: boolean;
  notificationSounds: boolean;
  plugins: Record<string, { volume: number; muted: boolean }>;
} = { volume: 100, muted: false, notificationSounds: true, plugins: {} };

/** Localized fallback label for a platform-native fixture pin. */
export function fixtureSpecialLabel(special: SpecialShortcut): string {
  return t(
    special === "computer"
      ? "settings.shortcuts.specialComputer"
      : "settings.shortcuts.specialTrash",
  );
}

export function updateConfig(path: string, value: unknown): void {
  if (path.startsWith("audio.")) {
    const parts = path.split(".");
    const pluginId = parts[1] === "plugins" ? parts[2] : undefined;
    const level =
      pluginId === undefined
        ? fixtureAudio
        : (fixtureAudio.plugins[pluginId] ??= { volume: 100, muted: false });
    const field = parts.at(-1);
    if (field === "volume") level.volume = asNumber(value, path);
    else if (field === "muted") level.muted = asBoolean(value, path);
    else if (field === "notificationSounds" && pluginId === undefined)
      fixtureAudio.notificationSounds = asBoolean(value, path);
    else throw new Error(`unsupported audio setting "${path}"`);
    return;
  }
  const store = useSmabar.getState();
  switch (path) {
    case "layout.position":
      store.setLayout({
        ...store.layout,
        position: asChoice(value, ["top", "bottom"] as const, path),
      });
      break;
    case "layout.variant":
      store.setLayout({
        ...store.layout,
        variant: asChoice(value, ["split", "rows", "solo"] as const, path),
      });
      break;
    case "layout.primaryZone":
      store.setLayout({
        ...store.layout,
        primaryZone: asChoice(value, ["shortcuts", "plugins"] as const, path),
      });
      break;
    case "layout.width":
      store.setLayout({
        ...store.layout,
        width: asChoice(value, ["full", "auto"] as const, path),
      });
      break;
    case "layout.margin":
      store.setLayout({ ...store.layout, margin: asNumber(value, path) });
      break;
    case "layout.dividerRatio":
      store.setDividerRatio(asNumber(value, path));
      break;
    case "layout.maxWidth":
      store.setLayout({ ...store.layout, maxWidth: asNumber(value, path) });
      break;
    case "layout.behavior":
      store.setLayout({
        ...store.layout,
        behavior: asChoice(
          value,
          ["reserve", "float", "autohide"] as const,
          path,
        ),
      });
      break;
    case "layout.yieldToFullscreen":
      store.setLayout({
        ...store.layout,
        yieldToFullscreen: asBoolean(value, path),
      });
      break;
    case "zOrder":
      store.setZOrder(asChoice(value, ["top", "bottom"] as const, path));
      break;
    case "shortcuts.labels":
      store.setShortcuts({
        ...store.shortcuts,
        labels: asChoice(value, ["right", "below", "hidden"] as const, path),
      });
      break;
    case "shortcuts.iconSize":
      store.setShortcuts({
        ...store.shortcuts,
        iconSize: asNumber(value, path),
      });
      break;
    case "shortcuts.labelSize":
      store.setShortcuts({
        ...store.shortcuts,
        labelSize: asNumber(value, path),
      });
      break;
    case "shortcuts.pinned":
      reorderPins(asEntries(value));
      break;
    case "pluginsHidden":
      store.setPluginsHidden(asStrings(value, path));
      break;
    case "pluginsDeactivated":
      setDeactivatedPlugins(asStrings(value, path));
      break;
    case "pluginOrder":
      store.setPluginOrder(asStrings(value, path));
      break;
    case "theme":
      activateTheme(asString(value, path));
      break;
    case "themeExportDir":
      // Accepted and dropped: browser-dev has no filesystem to export into.
      asString(value, path);
      break;
    case "appearance.barChrome":
      store.setAppearance({
        ...store.appearance,
        barChrome: asChoice(value, ["card", "flat"] as const, path),
      });
      break;
    case "appearance.tileChrome":
      store.setAppearance({
        ...store.appearance,
        tileChrome: asChoice(value, ["card", "flat"] as const, path),
      });
      break;
    case "appearance.shortcutAlign":
      store.setAppearance({
        ...store.appearance,
        shortcutAlign: asChoice(
          value,
          ["left", "center", "right"] as const,
          path,
        ),
      });
      break;
    case "appearance.pluginAlign":
      store.setAppearance({
        ...store.appearance,
        pluginAlign: asChoice(
          value,
          ["left", "center", "right"] as const,
          path,
        ),
      });
      break;
    case "appearance.pluginAccent":
      store.setAppearance({
        ...store.appearance,
        pluginAccent: asChoice(value, ["theme", "plugin"] as const, path),
      });
      break;
    case "appearance.tokens":
      store.setAppearance({
        ...store.appearance,
        tokens: asTokenMap(value, path),
      });
      break;
    case "popups.enabled":
      store.setPopups({ ...store.popups, enabled: asBoolean(value, path) });
      break;
    case "popups.position":
      store.setPopups({
        ...store.popups,
        position: asChoice(
          value,
          [
            "top-left",
            "top-center",
            "top-right",
            "bottom-left",
            "bottom-center",
            "bottom-right",
          ] as const,
          path,
        ),
      });
      break;
    case "effects.hoverMagnify.enabled":
      store.setEffects({
        ...store.effects,
        hoverMagnify: {
          ...store.effects.hoverMagnify,
          enabled: asBoolean(value, path),
        },
      });
      break;
    case "effects.hoverMagnify.scale":
      store.setEffects({
        ...store.effects,
        hoverMagnify: {
          ...store.effects.hoverMagnify,
          scale: asNumber(value, path),
        },
      });
      break;
    case "effects.hoverMagnify.neighbors":
      store.setEffects({
        ...store.effects,
        hoverMagnify: {
          ...store.effects.hoverMagnify,
          neighbors: asNumber(value, path),
        },
      });
      break;
    case "effects.hoverPeek.enabled":
      store.setEffects({
        ...store.effects,
        hoverPeek: {
          ...store.effects.hoverPeek,
          enabled: asBoolean(value, path),
        },
      });
      break;
    case "language":
      store.setLanguage(asString(value, path));
      break;
    case "settingsWindow": {
      if (typeof value !== "object" || value === null || Array.isArray(value)) {
        throw new Error('fixture: "settingsWindow" must be an object');
      }
      const size = value as Record<string, unknown>;
      store.setSettingsWindow({
        width: asNumber(size.width, "settingsWindow.width"),
        height: asNumber(size.height, "settingsWindow.height"),
        x:
          size.x === undefined || size.x === null
            ? null
            : asNumber(size.x, "settingsWindow.x"),
        y:
          size.y === undefined || size.y === null
            ? null
            : asNumber(size.y, "settingsWindow.y"),
      });
      break;
    }
    // The MCP endpoint has no shell state of its own; the System panel reads
    // it back through get_system_settings.
    case "mcp.enabled":
      fixtureSystem.mcp.enabled = asBoolean(value, path);
      break;
    case "mcp.port":
      fixtureSystem.mcp.port = asNumber(value, path);
      break;
    case "rendering":
      fixtureSystem.rendering.mode = asChoice(
        value,
        ["auto", "native", "software"],
        path,
      );
      break;
    case "effects.hoverPeek.delayMs":
      store.setEffects({
        ...store.effects,
        hoverPeek: {
          ...store.effects.hoverPeek,
          delayMs: asNumber(value, path),
        },
      });
      break;
    default:
      // plugins.<id>.<key> is the one open-ended path: the keys come from a
      // plugin's own settingsSchema, so it cannot be enumerated here. The
      // store update mirrors what the core would echo back.
      if (path.startsWith("plugins.")) {
        const [, pluginId, key] = path.split(".");
        if (pluginId === undefined || key === undefined) {
          throw new Error(`fixture: incomplete plugin config path "${path}"`);
        }
        store.patchPluginSetting(pluginId, key, value);
        break;
      }
      throw new Error(`no browser-dev fixture for config path "${path}"`);
  }
}

/**
 * Mirrors the core's theme activation: the RESOLVED token map (bundled
 * default overlaid with the theme's tokens — never the bare overlay, which
 * would strip every other token off :root and break the design) lands on
 * the root, and the settings block runs one-shot through
 * {@link updateConfig}.
 */
function activateTheme(name: string): void {
  const theme = getFixtureTheme(name);
  if (!theme) throw new Error(`fixture: unknown theme "${name}"`);
  useSmabar.getState().setTheme(name);
  const appearance = useSmabar.getState().appearance;
  useSmabar.getState().setAppearance({ ...appearance, tokens: {} });
  applyTokenOverrides({});
  applyTheme({ ...DEFAULT_THEME.tokens, ...theme.tokens });
  for (const [path, value] of Object.entries(theme.settings)) {
    updateConfig(path, value);
  }
}

/**
 * Rebuilds the resolved pins from the new entry list; icons stay cached by id.
 *
 * The label is re-derived rather than carried over, because it is what a
 * rename changes: the core resolves `entry.label` FIRST and only then the
 * desktop entry's own name (shortcuts/service.rs), so reusing the previously
 * resolved pin would swallow both a new name and the clearing of one.
 */
function reorderPins(entries: ShortcutConfigEntry[]): void {
  const store = useSmabar.getState();
  const byId = new Map(store.shortcuts.pinned.map((pin) => [pin.id, pin]));
  const pinned = entries.map((entry): ResolvedShortcut => ({
    id: entry.id,
    label: entry.separator ? "" : resolvePinLabel(entry),
    icons: byId.get(entry.id)?.icons ?? [],
    desktopId: entry.desktopId,
    separator: entry.separator === true,
  }));
  store.setShortcuts({ ...store.shortcuts, pinned, entries });
}

/** A pin's own name, else the sample app's, in the core's precedence. */
function resolvePinLabel(entry: ShortcutConfigEntry): string {
  if (entry.label !== undefined && entry.label !== "") return entry.label;
  if (entry.special !== undefined) return fixtureSpecialLabel(entry.special);
  if (entry.url !== undefined) {
    return new URL(entry.url).hostname.replace(/^www\./, "");
  }
  // APPS derives every desktopId as `<lowercased name>.desktop`, so the name
  // comes back out of the id — no import from fixture.ts, no extra state.
  const stem = entry.desktopId?.replace(/\.desktop$/, "");
  if (stem === undefined || stem === "") return entry.id;
  return stem.charAt(0).toUpperCase() + stem.slice(1);
}

/**
 * Stands in for the supervisor: a plugin that is switched off loses its
 * tiles, one that is switched back on gets them again.
 */
function setDeactivatedPlugins(ids: string[]): void {
  const store = useSmabar.getState();
  const before = store.pluginsDeactivated;
  store.setPluginsDeactivated(ids);
  for (const id of ids) {
    if (!before.includes(id)) setDemoPluginRegistered(id, false);
  }
  for (const id of before) {
    if (!ids.includes(id)) setDemoPluginRegistered(id, true);
  }
}
