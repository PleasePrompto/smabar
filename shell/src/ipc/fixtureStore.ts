import { registerTile } from "../components/registry";
import { useSmabar, type InstalledPlugin } from "../store/bar";
import { installFixtureTheme } from "./fixtureThemes";
import type {
  StoreDetailInfo,
  StoreEntry,
  StoreKind,
  StoreOverview,
  StoreRelease,
} from "./store";

/**
 * Browser-dev stand-in for the core's Community Store: a six-entry catalog
 * that covers every listing state the settings panel renders, with install
 * and uninstall mutating it in memory so `list_plugins` and the bar reflect
 * what the panel did. No network, no progress events — a fixture install
 * is instant.
 */

const APP_VERSION = "0.2.0";
const HOST_OS = "linux";
const GENERATED_AT = "2026-08-30T06:00:00Z";
const FETCHED_AT = Date.parse("2026-09-03T07:30:00Z");

type Seed = Omit<StoreEntry, "incompatible" | "installable"> & {
  incompatible?: StoreEntry["incompatible"];
  readme: string;
  /** What the core's converter makes of `readme`; absent for a bare text. */
  readmeHtml?: string;
  releases: StoreRelease[];
  /** Accent the installed theme fixture gets, so it is visibly different. */
  accent?: string;
};

function author(login: string) {
  return { login, url: `https://github.com/${login}` };
}

function repo(
  nameWithOwner: string,
  license: string | null,
  stars: number,
  extra: Partial<StoreEntry["repo"]> = {},
): StoreEntry["repo"] {
  return {
    url: `https://github.com/${nameWithOwner}`,
    nameWithOwner,
    license,
    stars,
    archived: false,
    pushedAt: "2026-08-28T15:10:00Z",
    ...extra,
  };
}

const SEEDS: Seed[] = [
  {
    kind: "plugin",
    id: "github-notifications",
    name: "GitHub notifications",
    version: "1.3.0",
    description:
      "Unread GitHub notifications on the bar, grouped by repository, with a one-click mark-as-read.",
    keywords: ["github", "notifications", "developer"],
    author: author("mira-dev"),
    repo: repo("mira-dev/smabar-github-notifications", "MIT", 128),
    runtime: "python",
    requires: { smabar: "0.1.0", os: ["linux", "windows"], external: [] },
    path: ".",
    commit: "9f1c2e7a4b6d8c0e2f4a6b8c0d2e4f6a8b0c2d4e",
    ref: "v1.3.0",
    updatedAt: "2026-08-28T15:12:00Z",
    installed: {
      version: "1.2.0",
      commit: "3b7d9a1c5e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b",
      origin: "store",
      modified: false,
      deactivated: false,
      blocked: null,
    },
    update: { fromVersion: "1.2.0", toVersion: "1.3.0", contentChanged: false },
    blocked: null,
    readme:
      "# GitHub notifications\n\nShows your unread notifications on the bar.\n\n" +
      "## Setup\n\n1. Create a fine-grained token with the `notifications` scope.\n" +
      "2. Paste it into the plugin settings.\n\n" +
      "| Setting | Default |\n| --- | --- |\n| Poll interval | 60 s |\n\n" +
      "![Tile](docs/tile.png)\n\nSee the [changelog](CHANGELOG.md).",
    readmeHtml:
      "<h1>GitHub notifications</h1>\n<p>Shows your unread notifications on the bar.</p>\n" +
      "<h2>Setup</h2>\n<ol>\n<li>Create a fine-grained token with the <code>notifications</code> scope.</li>\n" +
      "<li>Paste it into the plugin settings.</li>\n</ol>\n" +
      "<table><thead><tr><th>Setting</th><th>Default</th></tr></thead>" +
      "<tbody><tr><td>Poll interval</td><td>60 s</td></tr></tbody></table>\n" +
      '<p><img src="https://raw.githubusercontent.com/mira-dev/smabar-github-notifications/9f1c2e7a4b6d8c0e2f4a6b8c0d2e4f6a8b0c2d4e/docs/tile.png" alt="Tile" /></p>\n' +
      '<p>See the <a href="https://github.com/mira-dev/smabar-github-notifications/blob/9f1c2e7a4b6d8c0e2f4a6b8c0d2e4f6a8b0c2d4e/CHANGELOG.md">changelog</a>.</p>\n',
    releases: [
      {
        version: "1.3.0",
        tag: "v1.3.0",
        commit: "9f1c2e7a4b6d8c0e2f4a6b8c0d2e4f6a8b0c2d4e",
        publishedAt: "2026-08-28T15:12:00Z",
      },
      {
        version: "1.2.0",
        tag: "v1.2.0",
        commit: "3b7d9a1c5e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b",
        publishedAt: "2026-07-02T09:00:00Z",
      },
    ],
  },
  {
    kind: "plugin",
    id: "docker-status",
    name: "Docker status",
    version: "0.6.2",
    description:
      "Running containers, their CPU and memory, and a stop button per container. Talks to the local Docker CLI.",
    keywords: ["docker", "containers", "devops"],
    author: author("containerkat"),
    repo: repo("containerkat/smabar-plugins", "GPL-3.0-only", 41),
    runtime: "exec",
    requires: { smabar: "0.2.0", os: ["linux"], external: ["docker"] },
    path: "plugins/docker-status",
    commit: "c4d6e8f0a2b4c6d8e0f2a4b6c8d0e2f4a6b8c0d2",
    ref: "docker-status-v0.6.2",
    updatedAt: "2026-08-20T11:40:00Z",
    installed: null,
    update: null,
    blocked: null,
    readme:
      "# Docker status\n\nNeeds the docker CLI on PATH and a user that may talk to the daemon (docker group on most distributions).",
    readmeHtml:
      "<h1>Docker status</h1>\n<p>Needs the docker CLI on PATH and a user that may talk to the daemon (docker group on most distributions).</p>\n",
    releases: [
      {
        version: "0.6.2",
        tag: "docker-status-v0.6.2",
        commit: "c4d6e8f0a2b4c6d8e0f2a4b6c8d0e2f4a6b8c0d2",
        publishedAt: "2026-08-20T11:40:00Z",
      },
    ],
  },
  {
    kind: "plugin",
    id: "spotify-lyrics",
    name: "Spotify lyrics",
    version: "2.1.0",
    description:
      "The current line of the song playing in Spotify, synced to the beat.",
    keywords: ["spotify", "music", "lyrics"],
    author: author("lyricbird"),
    repo: repo("lyricbird/smabar-spotify-lyrics", "Apache-2.0", 302),
    runtime: "python",
    requires: { smabar: "0.9.0", os: ["linux", "windows"], external: [] },
    path: ".",
    commit: "e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4d6e8f0",
    ref: "v2.1.0",
    updatedAt: "2026-09-01T18:05:00Z",
    installed: null,
    update: null,
    blocked: null,
    incompatible: ["minSmabar"],
    readme: "Spotify lyrics\n\nUses the media provider added in smabar 0.9.",
    releases: [
      {
        version: "2.1.0",
        tag: "v2.1.0",
        commit: "e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4d6e8f0",
        publishedAt: "2026-09-01T18:05:00Z",
      },
    ],
  },
  {
    kind: "plugin",
    id: "coin-flipper",
    name: "Coin flipper",
    version: "0.4.0",
    description: "Flip a coin from the bar. Heads or tails, nothing more.",
    keywords: ["fun", "random"],
    author: author("flipster"),
    repo: repo("flipster/coin-flipper", null, 3, {
      archived: true,
      pushedAt: "2026-03-14T08:00:00Z",
    }),
    runtime: "python",
    requires: { smabar: null, os: ["linux", "windows"], external: [] },
    path: ".",
    commit: "a8b0c2d4e6f8a0b2c4d6e8f0a2b4c6d8e0f2a4b6",
    ref: "v0.4.0",
    updatedAt: "2026-03-14T08:02:00Z",
    installed: null,
    update: null,
    blocked: {
      reason: "sends clipboard contents to a third-party server",
      version: "0.4.0",
    },
    incompatible: ["blocked"],
    readme: "Coin flipper\n\nFlips a coin.",
    releases: [
      {
        version: "0.4.0",
        tag: "v0.4.0",
        commit: "a8b0c2d4e6f8a0b2c4d6e8f0a2b4c6d8e0f2a4b6",
        publishedAt: "2026-03-14T08:02:00Z",
      },
    ],
  },
  {
    kind: "plugin",
    id: "pomodoro",
    name: "Pomodoro",
    version: "2.0.1",
    description:
      "A focus timer on the bar with a popup at the end of every interval.",
    keywords: ["timer", "focus", "productivity"],
    author: author("tomatoclock"),
    repo: repo("tomatoclock/smabar-pomodoro", "MIT", 77),
    runtime: "python",
    requires: { smabar: "0.1.0", os: ["linux", "windows"], external: [] },
    path: ".",
    commit: "b6c8d0e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4",
    ref: "v2.0.1",
    updatedAt: "2026-06-11T13:30:00Z",
    installed: {
      version: "2.0.1",
      commit: "b6c8d0e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4",
      origin: "store",
      modified: true,
      deactivated: false,
      blocked: null,
    },
    update: null,
    blocked: null,
    readme:
      "Pomodoro\n\n25 minutes of focus, 5 minutes of break. Intervals are configurable in the plugin settings.",
    releases: [
      {
        version: "2.0.1",
        tag: "v2.0.1",
        commit: "b6c8d0e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4",
        publishedAt: "2026-06-11T13:30:00Z",
      },
    ],
  },
  {
    kind: "theme",
    id: "nord",
    name: "Nord",
    version: "1.0.2",
    description:
      "The arctic, north-bluish palette for the bar and its flyouts.",
    keywords: ["theme", "dark", "blue"],
    author: author("frostpalette"),
    repo: repo("frostpalette/smabar-themes", "MIT", 56),
    runtime: null,
    requires: { smabar: null, os: [], external: [] },
    path: "themes/nord.json",
    commit: "d0e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4d6e8",
    ref: "nord-v1.0.2",
    updatedAt: "2026-07-19T10:00:00Z",
    installed: null,
    update: null,
    blocked: null,
    readme: "Nord for smabar\n\nBased on the Nord color palette.",
    releases: [
      {
        version: "1.0.2",
        tag: "nord-v1.0.2",
        commit: "d0e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4d6e8",
        publishedAt: "2026-07-19T10:00:00Z",
      },
    ],
    accent: "#88c0d0",
  },
];

interface Listing {
  entry: StoreEntry;
  readme: string;
  readmeHtml: string | null;
  releases: StoreRelease[];
  accent: string;
}

const key = (kind: StoreKind, id: string) => `${kind}:${id}`;

function seedListings(): Map<string, Listing> {
  return new Map(
    SEEDS.map(
      ({
        readme,
        readmeHtml,
        releases,
        accent,
        incompatible = [],
        ...rest
      }) => [
        key(rest.kind, rest.id),
        {
          entry: {
            ...rest,
            incompatible,
            installable: incompatible.length === 0,
          },
          readme,
          readmeHtml: readmeHtml ?? null,
          releases,
          accent: accent ?? "#4db6ac",
        },
      ],
    ),
  );
}

let listings = seedListings();
let fetchedAt = FETCHED_AT;

/** Test seam: back to the seeded catalog. */
export function resetFixtureStore(): void {
  listings = seedListings();
  fetchedAt = FETCHED_AT;
}

export function fixtureStoreOverview(): StoreOverview {
  return {
    catalogState: "fresh",
    generatedAt: GENERATED_AT,
    fetchedAt,
    lastError: null,
    appVersion: APP_VERSION,
    hostOs: HOST_OS,
    entries: [...listings.values()].map(({ entry }) => entry),
    pending: null,
  };
}

export function fixtureStoreRefresh(): StoreOverview {
  fetchedAt = Date.now();
  return fixtureStoreOverview();
}

function listing(kind: StoreKind, id: string): Listing {
  const found = listings.get(key(kind, id));
  if (found === undefined) {
    throw new Error(`no store listing "${kind}:${id}"`);
  }
  return found;
}

export function fixtureStoreDetail(
  kind: StoreKind,
  id: string,
): StoreDetailInfo {
  const { entry, readme, readmeHtml, releases } = listing(kind, id);
  return { entry, readme, readmeHtml, releases };
}

/** Mirrors the core's refusals, so the panel's error path is real here too. */
function assertInstallable(
  entry: StoreEntry,
  expectedVersion: string,
  confirmModified: boolean,
): void {
  if (entry.version !== expectedVersion) {
    throw new Error(
      `"${entry.id}" is listed as ${entry.version}, not ${expectedVersion}; refresh the catalog and try again`,
    );
  }
  if (!entry.installable) {
    throw new Error(
      `"${entry.id}" cannot be installed on this system (${entry.incompatible.join(", ")})`,
    );
  }
  if (entry.installed?.origin === "local") {
    throw new Error(
      `"${entry.id}" exists without an install receipt; remove the folder first`,
    );
  }
  if (entry.installed?.modified === true && !confirmModified) {
    throw new Error(
      `"${entry.id}" was modified locally; confirm to replace it (smabar keeps a backup)`,
    );
  }
}

function markInstalled(found: Listing): void {
  const { entry } = found;
  found.entry = {
    ...entry,
    installed: {
      version: entry.version,
      commit: entry.commit,
      origin: "store",
      modified: false,
      deactivated: false,
      blocked: null,
    },
    update: null,
  };
}

/** The tile a fixture-installed plugin shows; a real one pushes its own. */
function registerFixturePlugin(entry: StoreEntry): void {
  const store = useSmabar.getState();
  registerTile({
    id: `plugin:${entry.id}:main`,
    pluginId: entry.id,
    tile: { id: "main", name: entry.name },
    meta: { name: entry.name },
  });
  store.setPluginUi(
    `${entry.id}/main/tile`,
    `<div class="sb-tile"><span data-lucide="package"></span>${entry.name}</div>`,
  );
  store.bumpRegistryVersion();
}

export function fixtureInstallPlugin(
  id: string,
  expectedVersion: string,
  confirmModified: boolean,
): StoreOverview {
  const found = listing("plugin", id);
  assertInstallable(found.entry, expectedVersion, confirmModified);
  markInstalled(found);
  registerFixturePlugin(found.entry);
  return fixtureStoreOverview();
}

export function fixtureInstallTheme(
  name: string,
  expectedVersion: string,
): StoreOverview {
  const found = listing("theme", name);
  assertInstallable(found.entry, expectedVersion, false);
  markInstalled(found);
  installFixtureTheme(name, found.accent);
  return fixtureStoreOverview();
}

/**
 * The store's side of `remove_plugin` / `delete_theme`: forgets the receipt.
 * Ids the store never listed are not its business, so those are ignored.
 */
export function fixtureUninstall(kind: StoreKind, id: string): void {
  const found = listings.get(key(kind, id));
  if (found?.entry.installed === null || found === undefined) return;
  found.entry = { ...found.entry, installed: null, update: null };
}

/** Tiles for the entries the seed already lists as installed (DevFixture). */
export function seedFixtureStoreInstalls(): void {
  for (const { entry } of listings.values()) {
    if (entry.kind === "plugin" && entry.installed?.origin === "store") {
      registerFixturePlugin(entry);
    }
  }
}

/** The `list_plugins` rows of everything the fixture store installed. */
export function fixtureStorePlugins(): InstalledPlugin[] {
  const deactivated = useSmabar.getState().pluginsDeactivated;
  return [...listings.values()].flatMap(({ entry }): InstalledPlugin[] => {
    if (entry.kind !== "plugin" || entry.installed?.origin !== "store")
      return [];
    return [
      {
        id: entry.id,
        name: entry.name,
        description: entry.description,
        settingsSchema: null,
        tiles: [{ id: "main", name: entry.name }],
        status: deactivated.includes(entry.id) ? "deactivated" : "running",
        origin: "community",
        version: entry.installed.version,
        update: entry.update?.toVersion ?? null,
        modified: entry.installed.modified,
        blocked: entry.installed.blocked?.reason ?? null,
      },
    ];
  });
}
