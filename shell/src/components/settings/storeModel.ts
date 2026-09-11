/**
 * The Community Store's UI model — pure, so every product decision about
 * what a listing shows and offers is testable without a DOM.
 */
import { safeIntlLocale } from "../../i18n/t";
import type { StoreEntry } from "../../ipc/store";

/**
 * The one state a listing is in, read off the overview entry. The order of
 * the checks is the product decision: what is on disk outranks what the
 * catalog offers, and a listing that cannot be installed says why before it
 * says it is available.
 */
export type StoreState =
  /** A folder without a receipt: a User Plugin the store never touches. */
  | "local"
  /** The installed version is blocked; the plugin is disabled. */
  | "blocked"
  /** The installed files differ from what the store installed. */
  | "modified"
  | "updateAvailable"
  /** Same version, different files: the author republished. */
  | "contentChanged"
  | "current"
  /** Not installed and the listed version is blocked. */
  | "blockedListing"
  | "incompatible"
  | "available";

export type StoreAction = "install" | "update" | "uninstall";

export type BadgeTone = "neutral" | "ok" | "warn" | "danger" | "accent";

export function stateOf(entry: StoreEntry): StoreState {
  const { installed } = entry;
  if (installed !== null) {
    if (installed.origin === "local") return "local";
    if (installed.blocked !== null) return "blocked";
    if (installed.modified) return "modified";
    if (entry.update !== null) {
      return entry.update.contentChanged ? "contentChanged" : "updateAvailable";
    }
    return "current";
  }
  if (entry.blocked !== null) return "blockedListing";
  if (entry.incompatible.length > 0) return "incompatible";
  return "available";
}

/**
 * What the detail offers. Install and update follow the core's
 * `installable` verdict; uninstall is offered for everything the store put
 * on disk. A local folder gets nothing — the store leaves it alone.
 */
export function actionsFor(entry: StoreEntry): StoreAction[] {
  switch (stateOf(entry)) {
    case "local":
    case "blockedListing":
    case "incompatible":
      return [];
    case "available":
      return entry.installable ? ["install"] : [];
    case "current":
      return ["uninstall"];
    case "blocked":
    case "modified":
    case "updateAvailable":
    case "contentChanged":
      return entry.update !== null && entry.installable
        ? ["update", "uninstall"]
        : ["uninstall"];
  }
}

/** The card badge; `available` needs none — it is the default. */
export function badgeFor(
  state: StoreState,
): { key: string; tone: BadgeTone } | null {
  switch (state) {
    case "available":
      return null;
    case "local":
      return { key: "settings.store.stateLocal", tone: "neutral" };
    case "blocked":
    case "blockedListing":
      return { key: "settings.store.stateBlocked", tone: "danger" };
    case "modified":
      return { key: "settings.store.stateModified", tone: "warn" };
    case "updateAvailable":
      return { key: "settings.store.stateUpdate", tone: "accent" };
    case "contentChanged":
      return { key: "settings.store.stateContentChanged", tone: "warn" };
    case "current":
      return { key: "settings.store.stateCurrent", tone: "ok" };
    case "incompatible":
      return { key: "settings.store.stateIncompatible", tone: "neutral" };
  }
}

/** What the gear badge counts. */
export function countUpdates(entries: readonly StoreEntry[]): number {
  return entries.filter((entry) => entry.update !== null).length;
}

/** The seven characters GitHub shows. */
export function shortCommit(commit: string): string {
  return commit.slice(0, 7);
}

/** The exact listed source: the commit's tree, narrowed to the entry's path. */
export function commitUrl(entry: StoreEntry): string {
  const tree = `${entry.repo.url}/tree/${entry.commit}`;
  return entry.path === "." ? tree : `${tree}/${entry.path}`;
}

export function releaseUrl(repoUrl: string, tag: string): string {
  return `${repoUrl}/releases/tag/${tag}`;
}

/** An ISO date in the user's locale, or the raw string when it is not one. */
export function formatDate(date: string, language: string): string {
  const parsed = Date.parse(date);
  if (Number.isNaN(parsed)) return date;
  return new Intl.DateTimeFormat(safeIntlLocale(language), {
    dateStyle: "medium",
  }).format(parsed);
}

/** Download progress the way the update row shows it: "12.3 MB". */
export function formatMebibytes(bytes: number, language: string): string {
  return `${new Intl.NumberFormat(safeIntlLocale(language), {
    maximumFractionDigits: 1,
  }).format(bytes / 1_048_576)} MB`;
}

/**
 * Every whitespace-separated term must match somewhere in the name, id,
 * description or keywords — AND across terms, so "docker linux" narrows.
 */
export function filterEntries(
  entries: readonly StoreEntry[],
  query: string,
): StoreEntry[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return [...entries];
  return entries.filter((entry) => {
    const haystack = [
      entry.name,
      entry.id,
      entry.description,
      ...entry.keywords,
    ]
      .join("\n")
      .toLowerCase();
    return terms.every((term) => haystack.includes(term));
  });
}

/** Lower comes first: what needs a decision, then what is installed, then the rest. */
function rank(state: StoreState): number {
  switch (state) {
    case "updateAvailable":
    case "contentChanged":
    case "modified":
    case "blocked":
      return 0;
    case "current":
    case "local":
      return 1;
    case "available":
    case "incompatible":
    case "blockedListing":
      return 2;
  }
}

/** What the list can be ordered by; each key has the one direction that is useful. */
export type StoreSortKey = "state" | "name" | "stars" | "updatedAt";

/** What the list opens with: decisions first, then installed, then the rest. */
export const DEFAULT_STORE_SORT: StoreSortKey = "state";

function byName(a: StoreEntry, b: StoreEntry): number {
  return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
}

/** A date that does not parse sorts as the oldest, never as NaN. */
function time(date: string): number {
  const parsed = Date.parse(date);
  return Number.isNaN(parsed) ? 0 : parsed;
}

function compareBy(key: StoreSortKey, a: StoreEntry, b: StoreEntry): number {
  switch (key) {
    case "state":
      return rank(stateOf(a)) - rank(stateOf(b));
    case "name":
      return byName(a, b);
    case "stars":
      return b.repo.stars - a.repo.stars;
    case "updatedAt":
      return time(b.updatedAt) - time(a.updatedAt);
  }
}

/** Sorted by the chosen key; equal entries always fall back to the name. */
export function sortEntries(
  entries: readonly StoreEntry[],
  key: StoreSortKey = DEFAULT_STORE_SORT,
): StoreEntry[] {
  return [...entries].sort((a, b) => {
    const primary = compareBy(key, a, b);
    return primary !== 0 ? primary : byName(a, b);
  });
}

/** The star count the way GitHub shows it: "1.2k" past a thousand. */
export function formatStars(stars: number, language: string): string {
  return new Intl.NumberFormat(safeIntlLocale(language), {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(stars);
}
