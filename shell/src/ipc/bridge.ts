import { invoke } from "@tauri-apps/api/core";
import { listen, type Event as TauriEvent } from "@tauri-apps/api/event";

import { registerTile, unregisterPluginTiles } from "../components/registry";
import {
  pushNativePointerSample,
  pushPointerSample,
} from "../components/bar/useAutohide";
import { safeIntlLocale, setLocale } from "../i18n/t";
import {
  useSmabar,
  type AppearanceConfig,
  type EffectsConfig,
  type LayoutConfig,
  type PluginStatusInfo,
  type PopupsConfig,
  type RuntimeStatusInfo,
  type SettingsWindowConfig,
  type ShortcutsState,
  type ZOrder,
} from "../store/bar";
import { applyTheme } from "../theme/apply";
import { setAssetRoot } from "../plugins/assets";
import { setEmbedRoot } from "../plugins/embeds";
import { brandingStyle, setBranding } from "../plugins/branding";
import { POPUP_TOTAL_MAX } from "../plugins/popupQueue";
import type { PluginAddedPayload } from "../plugins/PluginContent";
import type { LegalChanged } from "./legal";
import { reportError, uiLog } from "./log";
import {
  initMemoryProbe,
  recordMemoryUi,
  suppressMemoryStateUpdate,
  type MemoryProbeMode,
} from "./memoryProbe";
import { initManagedPopups } from "./managedPopups";
import { showNotice, type SurfaceRole } from "./surface";

interface UiState {
  memoryProbe?: MemoryProbeMode | null;
  language: string;
  layout: LayoutConfig;
  zOrder: ZOrder;
  appearance: AppearanceConfig;
  popups: PopupsConfig;
  settingsWindow: SettingsWindowConfig;
  pluginsHidden: string[];
  pluginsDeactivated: string[];
  effects: EffectsConfig;
  shortcuts: ShortcutsState;
  locale: Record<string, string>;
  theme: Record<string, string>;
  themeName: string;
  pluginOrder: string[];
  plugins: Record<string, unknown>;
  dataRoot: string;
  embedRoot: string;
  /** The bundled terms of use are not accepted; see `legal-changed`. */
  legalRequired: boolean;
}

interface LocaleChanged {
  language: string;
  locale: Record<string, string>;
}

interface PluginStatusEvent {
  pluginId: string;
  status: PluginStatusInfo["status"];
  error?: string;
}

export interface PluginUiEvent {
  pluginId: string;
  tileId: string;
  target: "tile" | "flyout" | "hover" | "popup";
  html: string;
  /** Popup lifetime; missing/null = sticky until dismissed. */
  ttlMs?: number | null;
}

/**
 * The toast key for a runtime transition, or null when nothing should toast:
 * only the transition INTO `failed` notifies — repeated failure events and
 * every other state stay silent (the settings row is the full surface).
 */
export function runtimeFailureNotice(
  previous: RuntimeStatusInfo | null,
  next: RuntimeStatusInfo,
): string | null {
  if (next.state === "failed" && previous?.state !== "failed") {
    return "settings.system.runtimeFailedNotice";
  }
  return null;
}

const KEYBOARD_FOCUS_TARGETS =
  "input, select, textarea, button, iframe, a[href], [tabindex], [contenteditable], [role='combobox']";

export function needsKeyboardFocus(
  target: EventTarget | null | undefined,
): boolean {
  return (
    target instanceof HTMLElement &&
    (target.matches(KEYBOARD_FOCUS_TARGETS) ||
      target.closest('[role="menu"]') !== null)
  );
}

/**
 * Connects the shell to the Tauri core: initial UI state, live config
 * changes (locale/layout/shortcuts/effects) and the plugin lifecycle/UI
 * event streams.
 */
export async function initBridge(role: SurfaceRole = "bar"): Promise<void> {
  const keepsPluginHtml = role === "bar";
  // The bar is a DOCK-type X11 window; window managers never give docks
  // keyboard focus on click. When the user focuses something that needs
  // keys — form fields, custom selects, carousels and roving controls — ask
  // the core to focus the window explicitly, otherwise every keystroke
  // lands in the previously focused app.
  // focusin is composed, so inputs inside plugin shadow roots reach us too.
  if (role !== "settings") {
    window.addEventListener("focusin", (event) => {
      const target = event.composedPath()[0];
      if (needsKeyboardFocus(target)) {
        void invoke("focus_bar").catch(reportError);
      }
    });
  }

  // Subscribe before reading the snapshot. Updates received while the
  // listeners are being attached or the snapshot is in flight are replayed
  // afterwards, so startup cannot lose or overwrite a newer event.
  let startupUpdates: (() => void)[] | null = [];
  const afterStartup =
    <T>(apply: (payload: T) => void) =>
    (event: TauriEvent<T>) => {
      const update = () => {
        apply(event.payload);
      };
      if (startupUpdates === null) update();
      else startupUpdates.push(update);
    };
  let shortcutRefreshGeneration = 0;

  // Native presence also owns X11/Windows watchdog samples: moving a native
  // window can make WebKit replay a cached mouseover inside the hidden strip.
  if (role === "bar") {
    await listen<[number, number]>("pointer-entered-input-region", (event) => {
      pushNativePointerSample(...event.payload);
    });
    await listen("pointer-left-input-region", () => {
      pushNativePointerSample(Number.NaN, Number.NaN);
    });
    await listen<[number, number] | null>("bar-pointer-sample", (event) => {
      pushNativePointerSample(...(event.payload ?? [Number.NaN, Number.NaN]));
    });
    window.addEventListener("blur", () => {
      pushPointerSample(Number.NaN, Number.NaN);
    });
  }
  await listen<LocaleChanged>(
    "locale-changed",
    afterStartup((payload) => {
      setLocale(payload.locale);
      document.documentElement.lang = safeIntlLocale(payload.language);
      const current = useSmabar.getState();
      current.setLanguage(payload.language);
      current.bumpLocaleVersion();
    }),
  );
  await listen<{ layout: LayoutConfig }>(
    "layout-changed",
    afterStartup((payload) => {
      useSmabar.getState().setLayout(payload.layout);
    }),
  );
  await listen<{ zOrder: ZOrder }>(
    "z-order-changed",
    afterStartup((payload) => {
      useSmabar.getState().setZOrder(payload.zOrder);
    }),
  );
  // Accepting the terms in the settings window frees the bar as well.
  await listen<LegalChanged>(
    "legal-changed",
    afterStartup((payload) => {
      useSmabar.getState().setLegalRequired(payload.required);
    }),
  );
  // No payload: resolved shortcuts carry inline icon data URIs, far too
  // heavy to broadcast — refetch instead.
  await listen<unknown>(
    "shortcuts-changed",
    afterStartup(() => {
      const generation = ++shortcutRefreshGeneration;
      invoke<ShortcutsState>("get_shortcuts")
        .then((shortcuts) => {
          if (generation === shortcutRefreshGeneration) {
            useSmabar.getState().setShortcuts(shortcuts);
          }
        })
        .catch(reportError);
    }),
  );
  await listen<{ effects: EffectsConfig }>(
    "effects-changed",
    afterStartup((payload) => {
      useSmabar.getState().setEffects(payload.effects);
    }),
  );
  await listen<{ appearance: AppearanceConfig }>(
    "appearance-changed",
    afterStartup((payload) => {
      useSmabar.getState().setAppearance(payload.appearance);
    }),
  );
  await listen<{ popups: PopupsConfig }>(
    "popups-changed",
    afterStartup((payload) => {
      useSmabar.getState().setPopups(payload.popups);
    }),
  );
  await listen<{ settingsWindow: SettingsWindowConfig }>(
    "settings-window-changed",
    afterStartup((payload) => {
      useSmabar.getState().setSettingsWindow(payload.settingsWindow);
    }),
  );
  await listen<{ disabled: string[] }>(
    "plugins-hidden-changed",
    afterStartup((payload) => {
      useSmabar.getState().setPluginsHidden(payload.disabled);
    }),
  );
  // The bar drops a deactivated plugin's tiles through `plugin-removed`;
  // this only keeps the settings list in sync with what is switched off.
  await listen<{ deactivated: string[] }>(
    "plugins-deactivated-changed",
    afterStartup((payload) => {
      useSmabar.getState().setPluginsDeactivated(payload.deactivated);
    }),
  );
  await listen<{ theme: Record<string, string>; themeName: string }>(
    "theme-changed",
    afterStartup((payload) => {
      applyTheme(payload.theme);
      useSmabar.getState().setTheme(payload.themeName);
    }),
  );
  await listen<{ order: string[] }>(
    "plugin-order-changed",
    afterStartup((payload) => {
      useSmabar.getState().setPluginOrder(payload.order);
    }),
  );
  // Plugin lifecycle events maintain the tile registry; the bar re-renders
  // via registryVersion.
  await listen<PluginAddedPayload>(
    "plugin-added",
    afterStartup(registerPlugin),
  );
  await listen<{ pluginId: string }>(
    "plugin-removed",
    afterStartup((payload) => {
      const { pluginId } = payload;
      const gone = unregisterPluginTiles(pluginId);
      useSmabar.getState().dropPluginUi(pluginId, gone);
      useSmabar.getState().setPluginSchema(pluginId, null);
      useSmabar.getState().bumpRegistryVersion();
    }),
  );
  await listen<PluginStatusEvent>(
    "plugin-status",
    afterStartup((payload) => {
      const { pluginId, status, error } = payload;
      useSmabar.getState().setPluginStatus(pluginId, { status, error });
    }),
  );
  await listen<RuntimeStatusInfo>(
    "runtime-status",
    afterStartup((payload) => {
      const store = useSmabar.getState();
      const notice = runtimeFailureNotice(store.runtimeStatus, payload);
      store.setRuntimeStatus(payload);
      if (notice !== null && role === "bar") {
        void showNotice(notice).catch(reportError);
      }
    }),
  );
  // Stored raw; sanitization happens where it renders (ShadowHost).
  if (keepsPluginHtml || role === "notifications") {
    const applyUi = afterStartup<PluginUiEvent>((payload) => {
      routePluginUi(payload, role === "notifications", "live");
    });
    const channel = keepsPluginHtml ? `plugin-ui-${role}` : "plugin-ui";
    await listen<PluginUiEvent>(channel, (event) => {
      // Filter before startup queuing as well: notification windows never
      // retain the persistent HTML of every plugin in the background.
      if (keepsPluginHtml || event.payload.target === "popup") applyUi(event);
    });
  }

  if (role === "notifications") {
    await listen<{ key: string; ttlMs: number }>(
      "surface-notice",
      afterStartup((payload) => {
        useSmabar.getState().flashNotice(payload.key, payload.ttlMs);
      }),
    );
  }

  try {
    const ui = await invoke<UiState>("get_ui_state");
    initMemoryProbe(ui.memoryProbe, role);
    // Before queued plugin UI renders: sb-asset: is inert without it.
    setAssetRoot(ui.dataRoot);
    setEmbedRoot(ui.embedRoot);
    const store = useSmabar.getState();
    setLocale(ui.locale);
    document.documentElement.lang = safeIntlLocale(ui.language);
    applyTheme(ui.theme);
    store.setLayout(ui.layout);
    store.setZOrder(ui.zOrder);
    store.setLanguage(ui.language);
    store.setAppearance(ui.appearance);
    store.setPopups(ui.popups);
    store.setSettingsWindow(ui.settingsWindow);
    store.setShortcuts(ui.shortcuts);
    store.setPluginsHidden(ui.pluginsHidden);
    store.setPluginsDeactivated(ui.pluginsDeactivated);
    store.setEffects(ui.effects);
    store.setPluginOrder(ui.pluginOrder);
    store.setTheme(ui.themeName);
    store.setPluginSettings(ui.plugins);
    store.setLegalRequired(ui.legalRequired);
    store.bumpLocaleVersion();

    // The startup Added and UiRender events fired before this bridge existed —
    // fetch the current plugin list and the cached tile HTML once the
    // listeners are attached (slow-polling plugins would otherwise leave
    // empty tiles until their next push).
    const plugins = await invoke<PluginAddedPayload[]>("get_plugins");
    plugins.forEach(registerPlugin);
    if (keepsPluginHtml) {
      const rendered = await invoke<PluginUiEvent[]>("get_plugin_ui");
      rendered.forEach((render) => {
        routePluginUi(render, false, "snapshot");
      });
    }
    // Same replay rationale: a provisioning run triggered before the webview
    // attached emitted its transitions into the void.
    const runtime = await invoke<RuntimeStatusInfo>("get_runtime_status");
    useSmabar.getState().setRuntimeStatus(runtime);
  } finally {
    const queuedUpdates = startupUpdates;
    startupUpdates = null;
    queuedUpdates.forEach((update) => {
      update();
    });
  }
  if (role === "notifications") await initManagedPopups();
}

/** Live popups are ephemeral; startup cache replay only restores persistent UI. */
function routePluginUi(
  render: PluginUiEvent,
  allowPopup: boolean,
  source: "live" | "snapshot",
): void {
  const { pluginId, tileId, target, html, ttlMs } = render;
  recordMemoryUi(html.length, source);
  if (source === "live" && target !== "popup" && suppressMemoryStateUpdate())
    return;
  const store = useSmabar.getState();
  if (target === "popup") {
    if (allowPopup) {
      const full =
        store.popupQueue.visible.length + store.popupQueue.queued.length >=
        POPUP_TOTAL_MAX;
      store.enqueuePopup({ pluginId, tileId, html, ttlMs });
      if (full && store.popups.enabled) {
        uiLog(
          "warn",
          `popup queue reached ${String(POPUP_TOTAL_MAX)} items; the oldest waiting popup was discarded`,
          { pluginId },
        );
      }
    }
    return;
  }
  store.setPluginUi(`${pluginId}/${tileId}/${target}`, html);
}

function registerPlugin(plugin: PluginAddedPayload): void {
  useSmabar.getState().setPluginSchema(plugin.pluginId, {
    name: plugin.name,
    settingsSchema: plugin.settingsSchema,
  });
  // A reload re-emits this event with the CURRENT manifest, which may declare
  // FEWER tiles than before. Without the sweep the dropped tiles keep
  // rendering their last HTML until the app restarts — and one drag-reorder
  // while they are visible writes their ids into pluginOrder permanently.
  const declared = new Set(plugin.tiles.map((tile) => tile.id));
  const removed = unregisterPluginTiles(plugin.pluginId, declared);
  removed.forEach((tileId) => {
    setBranding(plugin.pluginId, tileId, undefined);
  });
  plugin.tiles.forEach((tile) => {
    // Popups reach the shell as bare ids through the popup queue, so the
    // branding is recorded here where the manifest is still in scope.
    setBranding(plugin.pluginId, tile.id, brandingStyle(tile));
    registerTile({
      id: `plugin:${plugin.pluginId}:${tile.id}`,
      pluginId: plugin.pluginId,
      iconDataUrl: plugin.iconDataUrl,
      tile,
      meta: { name: tile.name },
    });
  });
  const store = useSmabar.getState();
  if (removed.length > 0) {
    store.dropPluginUi(plugin.pluginId, removed);
    // A pinned flyout on a tile that no longer exists can never be closed
    // by the user, and peekFlyout refuses every hover preview while one is
    // pinned — the bar would look frozen.
    const openId = store.openFlyout;
    if (
      openId !== null &&
      removed.some((tileId) => openId === `plugin:${plugin.pluginId}:${tileId}`)
    ) {
      store.closeFlyout();
    }
  }
  store.bumpRegistryVersion();
}
