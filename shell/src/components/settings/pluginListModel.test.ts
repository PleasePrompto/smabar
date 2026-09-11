import { expect, test } from "vitest";

import type { InstalledPlugin } from "../../store/types";

import {
  buildRows,
  pluginOfTile,
  reorderableIds,
  type PluginTileEntry,
} from "./pluginListModel";

const tile = (id: string, name: string): PluginTileEntry => ({
  id,
  meta: { name },
});

const plugin = (
  id: string,
  status: InstalledPlugin["status"],
  extra: Partial<InstalledPlugin> = {},
): InstalledPlugin => ({
  id,
  name: id,
  description: null,
  settingsSchema: null,
  tiles: [],
  status,
  origin: "base",
  version: "0.1.0",
  update: null,
  modified: false,
  blocked: null,
  ...extra,
});

test("running plugins produce one reorderable row per tile, in bar order", () => {
  const rows = buildRows(
    [tile("plugin:clock:clock", "Clock"), tile("plugin:sys:cpu", "System")],
    [plugin("clock", "running"), plugin("sys", "running")],
    [],
  );
  expect(rows.map((row) => row.kind)).toEqual(["tile", "tile"]);
  expect(rows.map((row) => row.name)).toEqual(["Clock", "System"]);
  // orderIndex is the position in the full order, which is what a reorder
  // writes back — so it must follow the list, not the plugin.
  expect(
    rows.map((row) => (row.kind === "tile" ? row.orderIndex : -1)),
  ).toEqual([0, 1]);
});

test("a switched-off plugin keeps a row so it can be switched back on", () => {
  // This is the whole reason the list is not just the tile registry:
  // deactivating unregisters the tiles, so a registry-only list would lose
  // the plugin and with it the only way back.
  const rows = buildRows(
    [tile("plugin:clock:clock", "Clock")],
    [plugin("clock", "running"), plugin("weather", "deactivated")],
    [],
  );
  expect(rows).toHaveLength(2);
  const tail = rows[1];
  expect(tail?.kind).toBe("plugin");
  expect(tail?.kind === "plugin" && tail.state).toBe("off");
  expect(tail?.pluginId).toBe("weather");
});

test("a failed plugin is a row too, and carries its reason", () => {
  const rows = buildRows(
    [],
    [plugin("broken", "failed", { error: "manifest is not valid JSON" })],
    [],
  );
  const row = rows[0];
  expect(row?.kind === "plugin" && row.state).toBe("failed");
  expect(row?.kind === "plugin" && row.error).toBe(
    "manifest is not valid JSON",
  );
});

test("the dimmed tail comes last, so the reorderable rows stay contiguous", () => {
  // The drag maps a row index onto an order index directly; that only holds
  // while every tile row precedes every plugin-only row.
  const rows = buildRows(
    [tile("plugin:a:one", "A"), tile("plugin:b:one", "B")],
    [
      plugin("off1", "deactivated"),
      plugin("a", "running"),
      plugin("b", "running"),
    ],
    [],
  );
  const kinds = rows.map((row) => row.kind);
  expect(kinds).toEqual(["tile", "tile", "plugin"]);
  expect(kinds.lastIndexOf("tile")).toBeLessThan(kinds.indexOf("plugin"));
});

test("a plugin with two tiles gets two rows that know about each other", () => {
  const rows = buildRows(
    [tile("plugin:multi:a", "First"), tile("plugin:multi:b", "Second")],
    [plugin("multi", "running")],
    [],
  );
  expect(rows).toHaveLength(2);
  // Both rows carry the plugin's power and trash buttons, so the UI has to be
  // able to say that switching one off takes the other with it.
  expect(rows.every((row) => row.kind === "tile" && row.siblings === 2)).toBe(
    true,
  );
  // And the plugin must NOT also appear in the tail.
  expect(rows.some((row) => row.kind === "plugin")).toBe(false);
});

test("a hidden tile stays in place and stays reorderable", () => {
  const rows = buildRows(
    [tile("plugin:a:one", "A"), tile("plugin:b:one", "B")],
    [plugin("a", "running"), plugin("b", "running")],
    ["plugin:a:one"],
  );
  expect(rows[0]?.kind === "tile" && rows[0].hidden).toBe(true);
  expect(rows[1]?.kind === "tile" && rows[1].hidden).toBe(false);
  // Hiding is not deactivating: the row keeps its slot in the order.
  expect(reorderableIds(rows)).toEqual(["plugin:a:one", "plugin:b:one"]);
});

test("only tile rows are reorderable", () => {
  const rows = buildRows(
    [tile("plugin:a:one", "A")],
    [plugin("a", "running"), plugin("off1", "deactivated")],
    [],
  );
  expect(reorderableIds(rows)).toEqual(["plugin:a:one"]);
});

test("the plugin behind a namespaced tile id", () => {
  expect(pluginOfTile("plugin:weather:main")).toBe("weather");
  expect(pluginOfTile("plugin:a:b:c")).toBe("a");
  for (const invalid of ["clock", "plugin:incomplete", "plugin::main"]) {
    expect(() => pluginOfTile(invalid)).toThrow(
      `invalid plugin tile id: ${invalid}`,
    );
  }
});

test("every row knows where its plugin came from, once list_plugins answered", () => {
  const rows = buildRows(
    [tile("plugin:pomodoro:main", "Pomodoro"), tile("plugin:new:main", "New")],
    [
      plugin("pomodoro", "running", {
        origin: "community",
        version: "2.0.1",
        update: "2.1.0",
        modified: true,
      }),
      plugin("mine", "deactivated", { origin: "user", version: null }),
    ],
    [],
  );
  expect(rows[0]?.provenance).toEqual({
    origin: "community",
    version: "2.0.1",
    update: "2.1.0",
    modified: true,
    blocked: null,
  });
  // A tile that registered before list_plugins answered has no facts yet.
  expect(rows[1]?.provenance).toBeNull();
  expect(rows[2]?.kind).toBe("plugin");
  expect(rows[2]?.provenance).toEqual({
    origin: "user",
    version: null,
    update: null,
    modified: false,
    blocked: null,
  });
});
