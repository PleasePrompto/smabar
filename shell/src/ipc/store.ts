import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { useSmabar } from "../store/bar";
import { call } from "./call";
import { reportError } from "./log";

/**
 * The Community Store's contract with the core (`commands/store.rs`): one
 * cached overview of every catalog entry, details on demand, and mutating
 * commands that answer with the fresh overview. The catalog itself is
 * fetched and verified by the core on its own timer. The webview loads
 * presentation images directly from the permitted GitHub hosts.
 */

export type StoreKind = "plugin" | "theme";

/** `stale` still lists but the core refreshes before it installs. */
export type CatalogState = "fresh" | "stale" | "unavailable";

/** Why an entry cannot be installed on this system. */
export type Incompatibility =
  "os" | "minSmabar" | "basePlugin" | "userPlugin" | "blocked" | "bundledTheme";

export type InstallPhase =
  "downloading" | "verifying" | "installing" | "starting" | "done";

export interface StoreProgress {
  kind: StoreKind;
  id: string;
  phase: InstallPhase;
  received: number;
  /** Content-Length when GitHub sent one. */
  total: number | null;
}

/** A store block: the catalog names the reason and the version it hit. */
export interface StoreBlock {
  reason: string;
  version: string | null;
}

/** What is on disk for a listed id, as far as the receipt knows. */
export interface StoreInstalled {
  version: string;
  commit: string;
  /** `local` is a folder without a receipt — a User Plugin the store never touches. */
  origin: "store" | "local";
  modified: boolean;
  deactivated: boolean;
  blocked: StoreBlock | null;
}

export interface StoreUpdate {
  fromVersion: string;
  toVersion: string;
  /** The listed files differ from what was installed, version aside. */
  contentChanged: boolean;
}

export interface StoreEntry {
  kind: StoreKind;
  id: string;
  name: string;
  version: string;
  description: string;
  keywords: string[];
  icon: string | null;
  screenshots: string[];
  author: { login: string; url: string };
  repo: {
    url: string;
    nameWithOwner: string;
    license: string | null;
    stars: number;
    archived: boolean;
    pushedAt: string | null;
  };
  /** Null for themes. */
  runtime: "python" | "exec" | null;
  requires: {
    smabar: string | null;
    os: ("linux" | "windows")[];
    external: string[];
  };
  /** `.`, `plugins/<id>` or `themes/<name>.json`. */
  path: string;
  /** The exact source commit and the tag or branch it was listed from. */
  commit: string;
  ref: string;
  updatedAt: string;
  installed: StoreInstalled | null;
  update: StoreUpdate | null;
  /** The LISTED version is blocked in the catalog. */
  blocked: StoreBlock | null;
  /** Empty means installable on this system. */
  incompatible: Incompatibility[];
  installable: boolean;
}

export interface StoreOverview {
  catalogState: CatalogState;
  generatedAt: string | null;
  /** Unix milliseconds of the last successful fetch. */
  fetchedAt: number | null;
  lastError: string | null;
  appVersion: string;
  hostOs: string | null;
  entries: StoreEntry[];
  /** The one install the core is running right now. */
  pending: StoreProgress | null;
}

export interface StoreRelease {
  version: string;
  tag: string;
  commit: string;
  publishedAt: string | null;
}

export interface StoreDetailInfo {
  entry: StoreEntry;
  /** The README's markdown source, as the catalog delivered it. */
  readme: string | null;
  /**
   * The README rendered by the core's own converter: raw HTML dropped, links
   * absolute, images only from GitHub hosts. Null when there is no README
   * or the core predates the converter — the shell then shows `readme` as text.
   */
  readmeHtml: string | null;
  releases: StoreRelease[];
}

export interface StoreChanged {
  reason: "refresh" | "install" | "remove";
  catalogState: CatalogState;
}

/** The cached overview; no network. */
export function storeOverview(): Promise<StoreOverview> {
  return readOverview("store_overview");
}

/** Fetches the catalog again and answers with the overview it produced. */
export function storeRefresh(): Promise<StoreOverview> {
  return readOverview("store_refresh");
}

let overviewRead = 0;
async function readOverview(
  command: string,
  args?: Record<string, unknown>,
): Promise<StoreOverview> {
  const request = ++overviewRead;
  const overview = await call<StoreOverview>(command, args);
  if (request === overviewRead)
    useSmabar
      .getState()
      .setCommunityUpdates(
        overview.entries.filter((entry) => entry.update !== null),
      );
  return overview;
}

export function storeDetail(
  kind: StoreKind,
  id: string,
): Promise<StoreDetailInfo> {
  return call<StoreDetailInfo>("store_detail", { kind, id });
}

/**
 * Installs or updates one plugin to the version the user saw. A locally
 * modified plugin is only replaced with `confirmModified`.
 */
export function installStorePlugin(
  id: string,
  expectedVersion: string,
  options: { confirmModified: boolean },
): Promise<StoreOverview> {
  return readOverview("store_install_plugin", {
    id,
    expectedVersion,
    confirmModified: options.confirmModified,
  });
}

export function installStoreTheme(
  name: string,
  expectedVersion: string,
): Promise<StoreOverview> {
  return readOverview("store_install_theme", { name, expectedVersion });
}

/** Outside the Tauri window there is no event bus; nothing to stop either. */
const NO_EVENTS: Promise<UnlistenFn> = Promise.resolve(() => undefined);

const inTauri = () => "__TAURI_INTERNALS__" in window;

export function onStoreChanged(
  handler: (change: StoreChanged) => void,
): Promise<UnlistenFn> {
  if (!inTauri()) return NO_EVENTS;
  return listen<StoreChanged>("store-changed", (event) => {
    handler(event.payload);
  });
}

export function onStoreProgress(
  handler: (progress: StoreProgress) => void,
): Promise<UnlistenFn> {
  if (!inTauri()) return NO_EVENTS;
  return listen<StoreProgress>("store-progress", (event) => {
    handler(event.payload);
  });
}

/**
 * Counts the Community Plugins with a newer catalog version into the store,
 * where the settings gear reads it. Never throws: the badge is passive, and
 * the core already logged why the overview failed.
 */
export async function refreshCommunityBadge(): Promise<void> {
  try {
    await storeOverview();
  } catch (error: unknown) {
    reportError(error);
  }
}
