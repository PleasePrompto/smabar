import { expect, test } from "vitest";

import type { StoreEntry } from "../../ipc/store";
import {
  actionsFor,
  badgeFor,
  commitUrl,
  filterEntries,
  formatStars,
  formatDate,
  formatMebibytes,
  releaseUrl,
  shortCommit,
  sortEntries,
  stateOf,
  type StoreSortKey,
} from "./storeModel";

const COMMIT = "9f1c2e7a4b6d8c0e2f4a6b8c0d2e4f6a8b0c2d4e";

function entry(id: string, extra: Partial<StoreEntry> = {}): StoreEntry {
  return {
    kind: "plugin",
    id,
    name: id,
    version: "1.0.0",
    description: "",
    keywords: [],
    author: { login: "someone", url: "https://github.com/someone" },
    repo: {
      url: `https://github.com/someone/${id}`,
      nameWithOwner: `someone/${id}`,
      license: "MIT",
      stars: 0,
      archived: false,
      pushedAt: null,
    },
    runtime: "python",
    requires: { smabar: null, os: ["linux", "windows"], external: [] },
    path: ".",
    commit: COMMIT,
    ref: "v1.0.0",
    updatedAt: "2026-08-01T00:00:00Z",
    installed: null,
    update: null,
    blocked: null,
    incompatible: [],
    installable: true,
    ...extra,
  };
}

const installed = (
  extra: Partial<NonNullable<StoreEntry["installed"]>> = {},
): NonNullable<StoreEntry["installed"]> => ({
  version: "1.0.0",
  commit: COMMIT,
  origin: "store",
  modified: false,
  deactivated: false,
  blocked: null,
  ...extra,
});

const BLOCK = { reason: "phones home", version: "1.0.0" };

test("the state reads what is on disk before what the catalog offers", () => {
  expect(stateOf(entry("a"))).toBe("available");
  expect(stateOf(entry("a", { installed: installed() }))).toBe("current");
  expect(
    stateOf(entry("a", { installed: installed({ origin: "local" }) })),
  ).toBe("local");
  expect(
    stateOf(entry("a", { installed: installed({ blocked: BLOCK }) })),
  ).toBe("blocked");
  expect(
    stateOf(
      entry("a", {
        installed: installed({ modified: true }),
        update: {
          fromVersion: "1.0.0",
          toVersion: "1.1.0",
          contentChanged: false,
        },
      }),
    ),
  ).toBe("modified");
  expect(
    stateOf(
      entry("a", {
        installed: installed(),
        update: {
          fromVersion: "1.0.0",
          toVersion: "1.1.0",
          contentChanged: false,
        },
      }),
    ),
  ).toBe("updateAvailable");
  expect(
    stateOf(
      entry("a", {
        installed: installed(),
        update: {
          fromVersion: "1.0.0",
          toVersion: "1.0.0",
          contentChanged: true,
        },
      }),
    ),
  ).toBe("contentChanged");
  expect(
    stateOf(entry("a", { blocked: BLOCK, incompatible: ["blocked"] })),
  ).toBe("blockedListing");
  expect(stateOf(entry("a", { incompatible: ["minSmabar"] }))).toBe(
    "incompatible",
  );
  // A local folder outranks everything else the entry says.
  expect(
    stateOf(
      entry("a", {
        installed: installed({
          origin: "local",
          modified: true,
          blocked: BLOCK,
        }),
        update: {
          fromVersion: "1.0.0",
          toVersion: "1.1.0",
          contentChanged: true,
        },
      }),
    ),
  ).toBe("local");
});

test("actions: install only when installable, update only with a listed update", () => {
  expect(actionsFor(entry("a"))).toEqual(["install"]);
  expect(actionsFor(entry("a", { installable: false }))).toEqual([]);
  expect(
    actionsFor(entry("a", { incompatible: ["os"], installable: false })),
  ).toEqual([]);
  expect(
    actionsFor(
      entry("a", {
        blocked: BLOCK,
        incompatible: ["blocked"],
        installable: false,
      }),
    ),
  ).toEqual([]);
  expect(actionsFor(entry("a", { installed: installed() }))).toEqual([
    "uninstall",
  ]);
  expect(
    actionsFor(entry("a", { installed: installed({ origin: "local" }) })),
  ).toEqual([]);
  const update = {
    fromVersion: "1.0.0",
    toVersion: "1.1.0",
    contentChanged: false,
  };
  expect(actionsFor(entry("a", { installed: installed(), update }))).toEqual([
    "update",
    "uninstall",
  ]);
  expect(
    actionsFor(
      entry("a", { installed: installed({ modified: true }), update }),
    ),
  ).toEqual(["update", "uninstall"]);
  expect(
    actionsFor(entry("a", { installed: installed({ modified: true }) })),
  ).toEqual(["uninstall"]);
  expect(
    actionsFor(
      entry("a", { installed: installed({ blocked: BLOCK }), update }),
    ),
  ).toEqual(["update", "uninstall"]);
  // The core's verdict wins: an update it refuses is not offered.
  expect(
    actionsFor(
      entry("a", { installed: installed(), update, installable: false }),
    ),
  ).toEqual(["uninstall"]);
});

test("every state but available carries a badge key, blocks read as danger", () => {
  expect(badgeFor("available")).toBeNull();
  expect(badgeFor("current")).toEqual({
    key: "settings.store.stateCurrent",
    tone: "ok",
  });
  expect(badgeFor("blocked")?.tone).toBe("danger");
  expect(badgeFor("blockedListing")?.tone).toBe("danger");
  expect(badgeFor("modified")?.tone).toBe("warn");
  expect(badgeFor("contentChanged")?.tone).toBe("warn");
  expect(badgeFor("updateAvailable")?.key).toBe("settings.store.stateUpdate");
  expect(badgeFor("local")?.key).toBe("settings.store.stateLocal");
  expect(badgeFor("incompatible")?.tone).toBe("neutral");
});

test("links name the exact commit and narrow to the entry's path", () => {
  expect(shortCommit(COMMIT)).toBe("9f1c2e7");
  expect(commitUrl(entry("a"))).toBe(
    `https://github.com/someone/a/tree/${COMMIT}`,
  );
  expect(commitUrl(entry("a", { path: "plugins/a" }))).toBe(
    `https://github.com/someone/a/tree/${COMMIT}/plugins/a`,
  );
  expect(commitUrl(entry("nord", { path: "themes/nord.json" }))).toBe(
    `https://github.com/someone/nord/tree/${COMMIT}/themes/nord.json`,
  );
  expect(releaseUrl("https://github.com/someone/a", "a-v1.0.0")).toBe(
    "https://github.com/someone/a/releases/tag/a-v1.0.0",
  );
});

test("dates render in the locale, anything else passes through", () => {
  expect(formatDate("2026-08-28T15:12:00Z", "en")).toBe("Aug 28, 2026");
  expect(formatDate("2026-08-28T15:12:00Z", "de")).toBe("28.08.2026");
  expect(formatDate("not a date", "en")).toBe("not a date");
  expect(formatMebibytes(1_048_576 * 1.25, "en")).toBe("1.3 MB");
});

test("search ANDs every term across name, id, description and keywords", () => {
  const entries = [
    entry("docker-status", {
      name: "Docker status",
      description: "Talks to the local Docker CLI.",
      keywords: ["containers", "devops"],
    }),
    entry("spotify-lyrics", {
      name: "Spotify lyrics",
      description: "The current line of the song.",
      keywords: ["music"],
    }),
  ];
  const ids = (query: string) =>
    filterEntries(entries, query).map((found) => found.id);
  expect(ids("")).toEqual(["docker-status", "spotify-lyrics"]);
  expect(ids("  ")).toEqual(["docker-status", "spotify-lyrics"]);
  expect(ids("DOCKER")).toEqual(["docker-status"]);
  expect(ids("devops")).toEqual(["docker-status"]);
  expect(ids("song")).toEqual(["spotify-lyrics"]);
  expect(ids("docker cli")).toEqual(["docker-status"]);
  expect(ids("docker music")).toEqual([]);
  expect(ids("status-")).toEqual([]);
});

test("sorting: what needs a decision, then installed, then the rest, by name", () => {
  const update = {
    fromVersion: "1.0.0",
    toVersion: "1.1.0",
    contentChanged: false,
  };
  const sorted = sortEntries([
    entry("zeta"),
    entry("beta", { installed: installed() }),
    entry("alpha"),
    entry("update", { installed: installed(), update }),
    entry("Blocked", {
      blocked: BLOCK,
      incompatible: ["blocked"],
      installable: false,
    }),
    entry("modified", { installed: installed({ modified: true }) }),
    entry("local", { installed: installed({ origin: "local" }) }),
  ]).map((found) => found.id);
  expect(sorted).toEqual([
    "modified",
    "update",
    "beta",
    "local",
    "alpha",
    "Blocked",
    "zeta",
  ]);
});

test("each key sorts its one useful way and ties fall back to the name", () => {
  const entries = [
    entry("b", {
      name: "Beta",
      updatedAt: "2026-01-02T00:00:00Z",
      repo: { ...entry("b").repo, stars: 40 },
    }),
    entry("a", { name: "alpha", updatedAt: "not a date" }),
    entry("c", {
      name: "Gamma",
      updatedAt: "2026-03-01T00:00:00Z",
      repo: { ...entry("c").repo, stars: 40 },
    }),
  ];
  const ids = (key: StoreSortKey) =>
    sortEntries(entries, key).map((found) => found.id);
  expect(ids("name")).toEqual(["a", "b", "c"]);
  // Most stars first; the two at 40 tie on the name.
  expect(ids("stars")).toEqual(["b", "c", "a"]);
  // Newest first; an unparsable date is the oldest, never NaN.
  expect(ids("updatedAt")).toEqual(["c", "b", "a"]);
});

test("stars read the way GitHub shows them", () => {
  expect(formatStars(0, "en")).toBe("0");
  expect(formatStars(987, "en")).toBe("987");
  expect(formatStars(1234, "en")).toBe("1.2K");
  expect(formatStars(1234, "de")).toBe("1234");
});
