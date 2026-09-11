import { expect, test } from "vitest";

import {
  getTiles,
  registerTile,
  sortTiles,
  unregisterPluginTiles,
  visibleTiles,
  type PluginTileDefinition,
} from "./registry";

const tile = (id: string): PluginTileDefinition => ({
  id,
  pluginId: id.split(":")[1] ?? "test",
  tile: { id: id.split(":")[2] ?? id, name: id },
  meta: { name: id },
});

const ids = (tiles: PluginTileDefinition[]): string[] => tiles.map((w) => w.id);

const registered = [
  tile("plugin:weather:main"),
  tile("plugin:system:main"),
  tile("plugin:clock:clock"),
  tile("plugin:hello:main"),
];

test("empty order keeps registration order", () => {
  expect(ids(sortTiles(registered, []))).toEqual([
    "plugin:weather:main",
    "plugin:system:main",
    "plugin:clock:clock",
    "plugin:hello:main",
  ]);
});

test("listed ids come first in list order, rest keeps registration order", () => {
  const order = ["plugin:hello:main", "plugin:clock:clock"];
  expect(ids(sortTiles(registered, order))).toEqual([
    "plugin:hello:main",
    "plugin:clock:clock",
    "plugin:weather:main",
    "plugin:system:main",
  ]);
});

test("unknown ids in the order are ignored", () => {
  const order = ["ghost", "plugin:system:main"];
  expect(ids(sortTiles(registered, order))).toEqual([
    "plugin:system:main",
    "plugin:weather:main",
    "plugin:clock:clock",
    "plugin:hello:main",
  ]);
});

test("a full order list rearranges everything", () => {
  const order = [
    "plugin:clock:clock",
    "plugin:weather:main",
    "plugin:hello:main",
    "plugin:system:main",
  ];
  expect(ids(sortTiles(registered, order))).toEqual(order);
});

test("visibleTiles drops disabled ids and keeps the rest in order", () => {
  const disabled = ["plugin:clock:clock", "plugin:hello:main"];
  expect(ids(visibleTiles(registered, disabled))).toEqual([
    "plugin:weather:main",
    "plugin:system:main",
  ]);
});

test("visibleTiles ignores unknown disabled ids", () => {
  expect(ids(visibleTiles(registered, ["ghost"]))).toEqual(ids(registered));
  expect(ids(visibleTiles(registered, []))).toEqual(ids(registered));
});

test("re-registering a plugin drops only the tiles it no longer declares", () => {
  registerTile(tile("plugin:ai:claude"));
  registerTile(tile("plugin:ai:codex"));
  registerTile(tile("plugin:other:main"));

  const removed = unregisterPluginTiles("ai", new Set(["codex"]));

  expect(removed).toEqual(["claude"]);
  const remaining = ids(getTiles());
  expect(remaining).toContain("plugin:ai:codex");
  expect(remaining).toContain("plugin:other:main");
  expect(remaining).not.toContain("plugin:ai:claude");
});

test("a surviving tile keeps its slot instead of moving to the end", () => {
  // Registry insertion order is stable: deleting and re-adding a survivor
  // would reshuffle tiles across plugins on every hot reload.
  registerTile(tile("plugin:a:one"));
  registerTile(tile("plugin:b:two"));
  registerTile(tile("plugin:a:three"));

  unregisterPluginTiles("a", new Set(["one"]));

  // The registry is module-level, so compare only these three ids.
  const order = ids(getTiles()).filter(
    (id) => id.startsWith("plugin:a:") || id.startsWith("plugin:b:"),
  );
  expect(order).toEqual(["plugin:a:one", "plugin:b:two"]);
});

test("without a keep set every tile of that plugin goes", () => {
  registerTile(tile("plugin:gone:x"));
  registerTile(tile("plugin:gone:y"));

  expect(unregisterPluginTiles("gone").sort()).toEqual(["x", "y"]);
  expect(ids(getTiles())).not.toContain("plugin:gone:x");
});
