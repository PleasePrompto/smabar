import type { PopupRequest, PopupStackState } from "../plugins/popupQueue";
import type { StoreEntry, StoreKind } from "../ipc/store";
import type {
  AppearanceConfig,
  EffectsConfig,
  FlyoutId,
  FlyoutMode,
  FlyoutRect,
  LayoutConfig,
  PluginSchemaInfo,
  PluginStatusInfo,
  PopupsConfig,
  RuntimeStatusInfo,
  SettingsWindowConfig,
  ShortcutsState,
  UpdateStatus,
  UpdateInfo,
  ZOrder,
} from "./types";

/** Runtime state and actions exposed by the shell's single Zustand store. */
export interface SmabarState {
  layout: LayoutConfig;
  zOrder: ZOrder;
  /** Configured language code, used for locale-aware Intl formatting. */
  language: string;
  appearance: AppearanceConfig;
  popups: PopupsConfig;
  shortcuts: ShortcutsState;
  /** Tile ids hidden from the tile zone; their plugins keep running. */
  pluginsHidden: string[];
  /** Plugin ids the user switched off; their processes are stopped. */
  pluginsDeactivated: string[];
  effects: EffectsConfig;
  /** Configured tile order (ids first, rest keeps registration order). */
  pluginOrder: string[];
  /** Name of the active theme (`get_ui_state.themeName` / `theme-changed`). */
  theme: string;
  settingsGroup: string;
  settingsStoreEntry: { kind: StoreKind; id: string } | null;
  settingsOpen: boolean;
  settingsWindow: SettingsWindowConfig;
  dropActive: boolean;
  fileDrag: boolean;
  reordering: boolean;
  /** Natural content height (CSS px) of every rendered plugin cover, by
   *  tile id. The bar row grows to the tallest one (BarShell), so an
   *  over-tall cover widens the whitespace instead of being clipped. */
  coverHeights: Record<string, number>;
  /** Locale key of the transient toast message (null = hidden). */
  notice: string | null;
  themeImportPath: string | null;
  overlayOpen: boolean;
  /** At most one flyout is open globally; opening another replaces it. */
  openFlyout: FlyoutId | null;
  flyoutMode: FlyoutMode | null;
  triggerRect: FlyoutRect | null;
  popupQueue: PopupStackState;
  popupSequence: number;
  localeVersion: number;
  registryVersion: number;
  pluginStatus: Record<string, PluginStatusInfo>;
  runtimeStatus: RuntimeStatusInfo | null;
  /** The current runtime failure episode was already toasted. */
  runtimeFailureNoticed: boolean;
  updateStatus: UpdateStatus;
  updateChannel: "app" | "store" | null;
  /** Last confirmed offer survives a failed/background check and permits retry. */
  updateOffer: UpdateInfo | null;
  dismissedUpdateVersion: string | null;
  communityUpdates: StoreEntry[];
  /** The bundled terms of use are not accepted: the bar shows only the
   *  legal tile and the settings only the legal group. */
  legalRequired: boolean;
  /** Raw plugin HTML keyed "<pluginId>/<tileId>/<target>". */
  pluginUi: Record<string, string>;
  pluginSchemas: Record<string, PluginSchemaInfo>;
  pluginSettings: Record<string, unknown>;
  setLayout: (layout: LayoutConfig) => void;
  setZOrder: (zOrder: ZOrder) => void;
  setLanguage: (language: string) => void;
  setAppearance: (appearance: AppearanceConfig) => void;
  setPopups: (popups: PopupsConfig) => void;
  setDividerRatio: (ratio: number) => void;
  setShortcuts: (shortcuts: ShortcutsState) => void;
  setPluginsHidden: (ids: string[]) => void;
  setPluginsDeactivated: (ids: string[]) => void;
  setEffects: (effects: EffectsConfig) => void;
  setPluginOrder: (order: string[]) => void;
  setTheme: (theme: string) => void;
  setSettingsGroup: (group: string) => void;
  openStoreEntry: (kind: StoreKind, id: string) => void;
  setSettingsOpen: (open: boolean) => void;
  setSettingsWindow: (size: SettingsWindowConfig) => void;
  setDropActive: (active: boolean) => void;
  setFileDrag: (active: boolean) => void;
  setReordering: (active: boolean) => void;
  /** `null` forgets a cover (its tile left the bar). */
  setCoverHeight: (tileId: string, height: number | null) => void;
  setNotice: (key: string | null) => void;
  flashNotice: (key: string, ms?: number) => void;
  setThemeImportPath: (path: string | null) => void;
  setOverlayOpen: (open: boolean) => void;
  /** Idempotently opens a pinned flyout with a renderable anchor outside a drag. */
  openPinnedFlyout: (id: FlyoutId, rect: FlyoutRect) => boolean;
  toggleFlyout: (id: FlyoutId, rect: FlyoutRect) => void;
  /** `force` permits dedicated tile hover content when generic peek is off. */
  peekFlyout: (id: FlyoutId, rect: FlyoutRect, force?: boolean) => void;
  pinFlyout: () => void;
  closePeek: (id: FlyoutId) => void;
  closeFlyout: () => void;
  enqueuePopup: (request: PopupRequest, nowMs?: number) => void;
  dismissPopup: (id: number, nowMs?: number) => void;
  bumpLocaleVersion: () => void;
  bumpRegistryVersion: () => void;
  setPluginStatus: (pluginId: string, info: PluginStatusInfo) => void;
  setRuntimeStatus: (info: RuntimeStatusInfo) => void;
  markRuntimeFailureNoticed: () => void;
  setUpdateStatus: (status: UpdateStatus) => void;
  setCommunityUpdates: (entries: StoreEntry[]) => void;
  setLegalRequired: (required: boolean) => void;
  setPluginSchema: (pluginId: string, info: PluginSchemaInfo | null) => void;
  setPluginSettings: (settings: Record<string, unknown>) => void;
  patchPluginSetting: (pluginId: string, key: string, value: unknown) => void;
  setPluginUi: (key: string, html: string) => void;
  dropPluginUi: (pluginId: string, tileIds: string[]) => void;
}
