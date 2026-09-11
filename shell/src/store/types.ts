/**
 * Plain shapes of the bar's configuration state.
 *
 * Split out of `bar.ts` so the store file keeps room for behaviour; every
 * one of these is re-exported from `bar.ts`, which stays the single import
 * site for consumers.
 */

import type { PopupPosition } from "../plugins/popupQueue";
import type { PluginTileDef } from "../plugins/PluginContent";

/** Screen edge the bar docks to (mirrors `config::BarPosition`). */
export type BarPosition = "top" | "bottom";

/** Zone arrangement of the bar (mirrors `config::BarVariant`). */
export type BarVariant = "split" | "rows" | "solo";

/** The two bar zones (mirrors `config::ZoneKind`). */
export type ZoneKind = "shortcuts" | "plugins";

/** Horizontal extent of the bar (mirrors `config::BarWidth`). */
export type BarWidth = "full" | "auto";

/** Native window stacking requested by the user. */
export type ZOrder = "top" | "bottom";

/** Space reservation and edge-hide policy (mirrors `config::LayoutBehavior`). */
export type LayoutBehavior = "reserve" | "float" | "autohide";

/** Tile tile background treatment (global config or manifest override). */
export type TileChrome = "card" | "flat";

/**
 * Bar surface chrome (mirrors `config::BarChrome`). `flat` drops outline AND
 * shadow — the shadow's inset highlights would otherwise remain as a hairline.
 */
export type BarChrome = "card" | "flat";

/** Tile alignment inside a bar zone (mirrors `config::ZoneAlign`). */
export type ZoneAlign = "left" | "center" | "right";

export interface AppearanceConfig {
  barChrome: BarChrome;
  tileChrome: TileChrome;
  shortcutAlign: ZoneAlign;
  pluginAlign: ZoneAlign;
  /** Per-token theme overrides, applied on :root AFTER the active theme. */
  tokens: Record<string, string>;
}

export interface PopupsConfig {
  enabled: boolean;
  /** Screen corner/edge where the popup stack docks. */
  position: PopupPosition;
}

/**
 * Last user-selected settings-window geometry, in CSS pixels. `x`/`y` are
 * null until the window has been moved once; the core then centers it.
 */
export interface SettingsWindowConfig {
  width: number;
  height: number;
  x: number | null;
  y: number | null;
}

export interface MonitorPreference {
  id: string;
  label: string;
}

/** Bar layout config as delivered by `get_ui_state` / `layout-changed`. */
export interface LayoutConfig {
  /** Null follows the operating system's primary monitor. */
  monitor: MonitorPreference | null;
  position: BarPosition;
  variant: BarVariant;
  /** Validated config value; consumers apply {@link clampDividerRatio} defensively. */
  dividerRatio: number;
  primaryZone: ZoneKind;
  /** Full-width strip or content-sized centered dock. */
  width: BarWidth;
  /** Edge gap in px (auto width only), validated to 0–64. */
  margin: number;
  /** Full-width cap in px; zero is unlimited and auto width ignores it. */
  maxWidth: number;
  behavior: LayoutBehavior;
  /** Focused fullscreen applications may cover the bar. */
  yieldToFullscreen: boolean;
}

/** One pinned shortcut, resolved for display by the core. */
export interface ResolvedShortcut {
  id: string;
  label: string;
  /**
   * Tile image sources in preference order (one `data:` URI for local
   * icons, well-known icon URLs for website pins); empty = no icon.
   */
  icons: string[];
  desktopId?: string;
  separator: boolean;
}

/** Platform-native system location that can be pinned on every supported OS. */
export type SpecialShortcut = "computer" | "trash";

/**
 * One raw config pin (mirrors `config::ShortcutEntry`), index-aligned with
 * the resolved list — the settings panel reorders these and writes the array
 * back via `update_config shortcuts.pinned`.
 */
export interface ShortcutConfigEntry {
  id: string;
  desktopId?: string;
  path?: string;
  url?: string;
  special?: SpecialShortcut;
  label?: string;
  separator?: boolean;
}

/** Label placement of the shortcut tiles (mirrors `config::LabelMode`). */
export type LabelMode = "right" | "below" | "hidden";

/** A plugin's manifest name and settings schema, for the settings form. */
export interface PluginSchemaInfo {
  name: string;
  /** Raw JSON Schema from the manifest; absent when the plugin declares none. */
  settingsSchema?: unknown;
}

/** Preview colors of a resolved theme (mirrors `themes::ThemeColors`). */
export interface ThemeColors {
  accent: string;
  accent2: string;
  /** Main bar surface background. */
  surface: string;
  text: string;
}

export interface ThemeFont {
  family: string;
  source: string;
}

/** Optional self-describing metadata a theme file may carry. */
export interface ThemeMeta {
  /** Display name; the theme's identity stays the file stem. */
  name?: string;
  author?: string;
  version?: string;
  description?: string;
}

/**
 * One known theme for the settings picker (mirrors `themes::ThemeInfo`,
 * delivered by `list_themes`): compiled-in default or user drop-in.
 */
export interface ThemeSummary {
  name: string;
  source: "bundled" | "dropin";
  active: boolean;
  colors: ThemeColors;
  fonts: { sans: ThemeFont; mono: ThemeFont };
  meta?: ThemeMeta;
}

/** Shortcut zone state as delivered by `get_ui_state` / `get_shortcuts`. */
export interface ShortcutsState {
  pinned: ResolvedShortcut[];
  labels: LabelMode;
  /** Config value in px, validated to 16–64. */
  iconSize: number;
  /** Config value in px, validated to 9–16. */
  labelSize: number;
  entries: ShortcutConfigEntry[];
}

/** Visual effects config (`get_ui_state` / `effects-changed`). */
export interface EffectsConfig {
  hoverMagnify: {
    enabled: boolean;
    /** Validated config value; consumers apply {@link magnifyScale} defensively. */
    scale: number;
    /** Fisheye falloff span, validated to 0–3. */
    neighbors: number;
  };
  hoverPeek: {
    enabled: boolean;
    /** Config value validated to 100–2000 ms. */
    delayMs: number;
  };
}

/** Vertical opening direction of a flyout. */
export type FlyoutDirection = "down" | "up";

/** Identifies a flyout by its namespaced plugin tile registry id. */
export type FlyoutId = string;

/** A hover preview is passive; a pinned flyout owns the click catcher. */
export type FlyoutMode = "peek" | "pinned";

/** Screen-space rect of the trigger element (from getBoundingClientRect). */
export interface FlyoutRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Lifecycle state of a plugin (mirrors `plugins::PluginStatus`).
 * `deactivated` is the user's own off-switch: installed, intact, no process —
 * distinct from `stopped`, which is a plugin that was running and ended.
 */
export type PluginLifecycle =
  "starting" | "running" | "failed" | "stopped" | "deactivated";

/** Last lifecycle status the core reported for a plugin. */
export interface PluginStatusInfo {
  status: PluginLifecycle;
  error?: string;
}

/** Why runtime provisioning failed (mirrors `plugins::RuntimeFailureKind`). */
export type RuntimeFailureKind = "offline" | "uvMissing" | "other";

/** Managed Python runtime state (mirrors `plugins::RuntimeStatus`). */
export interface RuntimeStatusInfo {
  state: "absent" | "installing" | "ready" | "failed";
  /** installing: the last uv output line. */
  detail?: string | null;
  /** failed: the core's raw error message. */
  message?: string | null;
  kind?: RuntimeFailureKind | null;
}

/**
 * Where an installed plugin came from: shipped with smabar, installed by the
 * Community Store (it holds an install receipt), or a folder the user or an
 * agent created.
 */
export type PluginOrigin = "base" | "community" | "user";

/**
 * One INSTALLED plugin as `list_plugins` reports it — including the ones that
 * are deactivated or failed and therefore contribute no tiles. The settings
 * panel lists these; the bar itself only ever sees registered tiles.
 */
export interface InstalledPlugin {
  id: string;
  /** Manifest name; null when the manifest could not be loaded. */
  name: string | null;
  /** Manifest metadata remains available while the plugin is deactivated. */
  description: string | null;
  /** Optional normalized PNG from icon.* in the plugin folder. */
  iconDataUrl?: string | null;
  settingsSchema: unknown;
  tiles: PluginTileDef[];
  status: PluginLifecycle;
  /** Failure reason when `status` is `failed`. */
  error?: string | null;
  origin: PluginOrigin;
  /** Manifest version; null when the manifest declares none. */
  version: string | null;
  /** A newer version the Community Catalog lists; null when current. */
  update: string | null;
  /** The installed files differ from what the store installed. */
  modified: boolean;
  /** Reason when the store blocked the installed version; null otherwise. */
  blocked: string | null;
}

/** How this build applies a release; `null` shows it without a button. */
export type UpdateInstaller = "app" | "system" | null;

/** Application update lifecycle: the last check, or the install it started. */
export type UpdateStatus =
  | { state: "idle" | "checking" | "current" }
  | {
      state: "available";
      version: string;
      notes: string | null;
      /** The server's `pub_date` verbatim; formatted where it renders. */
      date: string | null;
      installer: UpdateInstaller;
    }
  | {
      state: "downloading";
      version: string;
      received: number;
      /** Content-Length when the server sent one. */
      total: number | null;
    }
  /** Verified; the installer is starting (Windows exits, macOS restarts). */
  | { state: "installing"; version: string }
  /** Linux: the package is on disk and the system installer has it. */
  | { state: "handedOff"; version: string; path: string; opened: boolean }
  | { state: "failed"; phase: "check" | "install"; message: string };
