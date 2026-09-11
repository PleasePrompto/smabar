import { create } from "zustand";

import {
  dismissPopup as removePopup,
  EMPTY_POPUP_STACK,
  enqueuePopup as appendPopup,
} from "../plugins/popupQueue";

export type { PopupPosition } from "../plugins/popupQueue";
// One import site for consumers: everything the store's shapes describe stays
// reachable from "../store/bar".
export type {
  AppearanceConfig,
  BarChrome,
  BarPosition,
  BarVariant,
  BarWidth,
  EffectsConfig,
  FlyoutDirection,
  FlyoutId,
  FlyoutMode,
  FlyoutRect,
  InstalledPlugin,
  LabelMode,
  LayoutBehavior,
  LayoutConfig,
  MonitorPreference,
  PluginLifecycle,
  PluginOrigin,
  PluginSchemaInfo,
  PluginStatusInfo,
  PopupsConfig,
  ResolvedShortcut,
  RuntimeFailureKind,
  RuntimeStatusInfo,
  SettingsWindowConfig,
  SpecialShortcut,
  ShortcutConfigEntry,
  ShortcutsState,
  ThemeColors,
  ThemeFont,
  ThemeMeta,
  ThemeSummary,
  TileChrome,
  UpdateInstaller,
  UpdateStatus,
  ZOrder,
  ZoneAlign,
  ZoneKind,
} from "./types";

import type { SmabarState } from "./barState";

// Pure bar-state helpers live in barModel.ts; re-exported so "../store/bar"
// stays the one import site.
export {
  BAR_MAX_WIDTH_MIN_PX,
  clampDividerRatio,
  clampHoverPeekDelay,
  clampMaxWidth,
  DIVIDER_DEFAULT,
  flyoutDirection,
  HOVER_PEEK_DELAY_DEFAULT_MS,
  HOVER_PEEK_DELAY_MAX_MS,
  HOVER_PEEK_DELAY_MIN_MS,
  isRenderableRect,
  magnifyScale,
  shortcutDisplayLabel,
} from "./barModel";

import {
  DIVIDER_DEFAULT,
  HOVER_PEEK_DELAY_DEFAULT_MS,
  isRenderableRect,
} from "./barModel";

/** Timer of the currently showing flashNotice; module-level like the toast it drives. */
let noticeTimer: number | undefined;

export const useSmabar = create<SmabarState>((set, get) => ({
  layout: {
    monitor: null,
    position: "bottom",
    variant: "split",
    dividerRatio: DIVIDER_DEFAULT,
    primaryZone: "plugins",
    width: "full",
    margin: 8,
    maxWidth: 0,
    behavior: "reserve",
    yieldToFullscreen: true,
  },
  zOrder: "top",
  language: "en",
  appearance: {
    barChrome: "card",
    tileChrome: "flat",
    shortcutAlign: "center",
    pluginAlign: "center",
    tokens: {},
  },
  popups: { enabled: true, position: "bottom-right" },
  shortcuts: {
    pinned: [],
    labels: "right",
    iconSize: 32,
    labelSize: 12,
    entries: [],
  },
  pluginsHidden: [],
  pluginsDeactivated: [],
  effects: {
    hoverMagnify: { enabled: true, scale: 1.25, neighbors: 2 },
    hoverPeek: { enabled: true, delayMs: HOVER_PEEK_DELAY_DEFAULT_MS },
  },
  pluginOrder: [],
  theme: "default",
  settingsGroup: "bar",
  settingsOpen: false,
  settingsWindow: { width: 960, height: 680, x: null, y: null },
  dropActive: false,
  fileDrag: false,
  reordering: false,
  coverHeights: {},
  notice: null,
  themeImportPath: null,
  overlayOpen: false,
  openFlyout: null,
  flyoutMode: null,
  triggerRect: null,
  popupQueue: EMPTY_POPUP_STACK,
  popupSequence: 0,
  // A layout switch invalidates every measured trigger rect and the solo
  // overlay, so both close.
  setLayout: (layout) => {
    set({
      layout,
      openFlyout: null,
      flyoutMode: null,
      triggerRect: null,
      overlayOpen: false,
    });
  },
  setZOrder: (zOrder) => {
    set({ zOrder });
  },
  setLanguage: (language) => {
    set({ language });
  },
  setAppearance: (appearance) => {
    set({ appearance });
  },
  setPopups: (popups) => {
    set((state) => ({
      popups,
      popupQueue: popups.enabled ? state.popupQueue : EMPTY_POPUP_STACK,
    }));
  },
  setDividerRatio: (ratio) => {
    set((s) => ({ layout: { ...s.layout, dividerRatio: ratio } }));
  },
  setShortcuts: (shortcuts) => {
    set({ shortcuts });
  },
  setPluginsHidden: (ids) => {
    set({ pluginsHidden: ids });
  },
  setPluginsDeactivated: (ids) => {
    set({ pluginsDeactivated: ids });
  },
  setEffects: (effects) => {
    set((state) =>
      !effects.hoverPeek.enabled && state.flyoutMode === "peek"
        ? {
            effects,
            openFlyout: null,
            flyoutMode: null,
            triggerRect: null,
          }
        : { effects },
    );
  },
  setPluginOrder: (order) => {
    set({ pluginOrder: order });
  },
  setTheme: (theme) => {
    set({ theme });
  },
  setSettingsGroup: (group) => {
    set({ settingsGroup: group });
  },
  setSettingsOpen: (open) => {
    set({ settingsOpen: open });
  },
  setSettingsWindow: (settingsWindow) => {
    set({ settingsWindow });
  },
  setDropActive: (active) => {
    set({ dropActive: active });
  },
  setFileDrag: (active) => {
    set({ fileDrag: active });
  },
  setReordering: (active) => {
    set(
      active
        ? {
            reordering: true,
            openFlyout: null,
            flyoutMode: null,
            triggerRect: null,
          }
        : { reordering: false },
    );
  },
  setCoverHeight: (tileId, height) => {
    // Plugins re-render every second; only a changed height may re-render
    // the bar shell.
    const current = get().coverHeights;
    if (height === null) {
      if (!(tileId in current)) return;
      set({
        coverHeights: Object.fromEntries(
          Object.entries(current).filter(([id]) => id !== tileId),
        ),
      });
      return;
    }
    if (current[tileId] === height) return;
    set({ coverHeights: { ...current, [tileId]: height } });
  },
  setNotice: (key) => {
    set({ notice: key });
  },
  flashNotice: (key, ms = 3000) => {
    set({ notice: key });
    window.clearTimeout(noticeTimer);
    noticeTimer = window.setTimeout(() => {
      // Guarded: a newer flash or an explicit setNotice must survive a
      // timer that was armed for an older key.
      if (get().notice === key) set({ notice: null });
    }, ms);
  },
  setThemeImportPath: (path) => {
    set({ themeImportPath: path });
  },
  setOverlayOpen: (open) => {
    set({ overlayOpen: open });
  },
  openPinnedFlyout: (id, rect) => {
    if (
      get().reordering ||
      !isRenderableRect(rect, window.innerWidth, window.innerHeight)
    ) {
      return false;
    }
    set({ openFlyout: id, flyoutMode: "pinned", triggerRect: rect });
    return true;
  },
  toggleFlyout: (id, rect) => {
    const state = get();
    if (state.openFlyout === id && state.flyoutMode === "pinned") {
      set({ openFlyout: null, flyoutMode: null, triggerRect: null });
    } else {
      state.openPinnedFlyout(id, rect);
    }
  },
  peekFlyout: (id, rect, force = false) => {
    const state = get();
    if (
      state.reordering ||
      (!state.effects.hoverPeek.enabled && !force) ||
      state.flyoutMode === "pinned" ||
      !isRenderableRect(rect, window.innerWidth, window.innerHeight)
    ) {
      return;
    }
    set({ openFlyout: id, flyoutMode: "peek", triggerRect: rect });
  },
  pinFlyout: () => {
    if (get().flyoutMode === "peek") set({ flyoutMode: "pinned" });
  },
  closePeek: (id) => {
    const state = get();
    if (state.openFlyout === id && state.flyoutMode === "peek") {
      set({ openFlyout: null, flyoutMode: null, triggerRect: null });
    }
  },
  closeFlyout: () => {
    set({ openFlyout: null, flyoutMode: null, triggerRect: null });
  },
  enqueuePopup: (request, nowMs = Date.now()) => {
    set((state) => {
      if (!state.popups.enabled) return state;
      const popupSequence = state.popupSequence + 1;
      const popupQueue = appendPopup(
        state.popupQueue,
        request,
        nowMs,
        popupSequence,
      );
      return { popupQueue, popupSequence };
    });
  },
  dismissPopup: (id, nowMs = Date.now()) => {
    set((state) => ({
      popupQueue: removePopup(state.popupQueue, id, nowMs),
    }));
  },
  localeVersion: 0,
  bumpLocaleVersion: () => {
    set((s) => ({ localeVersion: s.localeVersion + 1 }));
  },
  registryVersion: 0,
  bumpRegistryVersion: () => {
    set((s) => ({ registryVersion: s.registryVersion + 1 }));
  },
  pluginStatus: {},
  runtimeStatus: null,
  setRuntimeStatus: (info) => {
    set({ runtimeStatus: info });
  },
  updateStatus: { state: "idle" },
  updateChannel: null,
  setUpdateStatus: (status) => {
    set({ updateStatus: status });
  },
  communityUpdates: 0,
  setCommunityUpdates: (count) => {
    set({ communityUpdates: count });
  },
  legalRequired: false,
  setLegalRequired: (required) => {
    set({ legalRequired: required });
  },
  setPluginStatus: (pluginId, info) => {
    set((s) => ({ pluginStatus: { ...s.pluginStatus, [pluginId]: info } }));
  },
  pluginSchemas: {},
  setPluginSchema: (pluginId, info) => {
    set((s) => {
      if (info === null) {
        // Filter instead of delete: the key is dynamic, and rebuilding keeps
        // the store immutable anyway.
        return {
          pluginSchemas: Object.fromEntries(
            Object.entries(s.pluginSchemas).filter(([id]) => id !== pluginId),
          ),
        };
      }
      return { pluginSchemas: { ...s.pluginSchemas, [pluginId]: info } };
    });
  },
  pluginSettings: {},
  setPluginSettings: (settings) => {
    set({ pluginSettings: settings });
  },
  patchPluginSetting: (pluginId, key, value) => {
    set((s) => {
      const current = s.pluginSettings[pluginId];
      const base =
        typeof current === "object" &&
        current !== null &&
        !Array.isArray(current)
          ? (current as Record<string, unknown>)
          : {};
      return {
        pluginSettings: {
          ...s.pluginSettings,
          [pluginId]: { ...base, [key]: value },
        },
      };
    });
  },
  pluginUi: {},
  setPluginUi: (key, html) => {
    set((s) => ({ pluginUi: { ...s.pluginUi, [key]: html } }));
  },
  dropPluginUi: (pluginId, tileIds) => {
    const prefixes = tileIds.map((tileId) => `${pluginId}/${tileId}/`);
    set((s) => ({
      pluginUi: Object.fromEntries(
        Object.entries(s.pluginUi).filter(
          ([key]) => !prefixes.some((prefix) => key.startsWith(prefix)),
        ),
      ),
    }));
  },
}));
