// @vitest-environment happy-dom
import { expect, test } from "vitest";

import {
  isEditableTarget,
  resolveContextTargets,
  type ContextTarget,
} from "./contextTarget";
import {
  compactSeparators,
  isSubmenu,
  nextEnabledIndex,
  type ContextMenuItem,
} from "./model";
import {
  CONTEXT_MENU_MAX_ITEMS,
  CONTEXT_MENU_MAX_LABEL_CHARS,
  CONTEXT_MENU_MAX_SPEC_CHARS,
  CONTEXT_MENU_MAX_VALUE_CHARS,
  parseContextItems,
} from "./pluginMenu";
import { tileMenu } from "./menus";

const action = (id: string, extra: Partial<ContextMenuItem> = {}) =>
  ({
    id,
    label: id,
    command: { type: "open-settings" },
    ...extra,
  }) as ContextMenuItem;

test("keyboard navigation wraps and skips separators and disabled rows", () => {
  const items: ContextMenuItem[] = [
    action("a"),
    { id: "s", separator: true },
    action("b", { disabled: true }),
    action("c"),
  ];
  expect(nextEnabledIndex(items, -1, 1)).toBe(0);
  expect(nextEnabledIndex(items, 0, 1)).toBe(3);
  expect(nextEnabledIndex(items, 3, 1)).toBe(0);
  expect(nextEnabledIndex(items, 0, -1)).toBe(3);
  expect(nextEnabledIndex(items, items.length, -1)).toBe(3);
  expect(nextEnabledIndex([{ id: "s", separator: true }], -1, 1)).toBe(-1);
  expect(nextEnabledIndex([], -1, 1)).toBe(-1);
});

test("separators never dangle at an edge or double up", () => {
  const compact = compactSeparators([
    { id: "s1", separator: true },
    action("a"),
    { id: "s2", separator: true },
    { id: "s3", separator: true },
    action("b"),
    { id: "s4", separator: true },
  ]);
  expect(compact.map((item) => item.id)).toEqual(["a", "s2", "b"]);
  expect(compactSeparators([{ id: "s", separator: true }])).toEqual([]);
});

test("every tile menu includes its plugin lifecycle actions", () => {
  expect(tileMenu("plugin:weather:main").map((item) => item.id)).toEqual([
    "tile.hide",
    "tile.plugin.sep",
    "tile.plugin.deactivate",
    "tile.plugin.delete",
    "tile.sep",
    "tile.settings",
  ]);
});

test("a plugin spec becomes serializable allowlisted commands", () => {
  const items = parseContextItems(
    JSON.stringify([
      {
        action: "download",
        value: "https://x/1.png",
        label: "Save image",
        icon: "download",
      },
      { separator: true },
      { action: "mute", label: "Muted", checked: true },
      { action: "delete", label: "Delete", danger: true, disabled: true },
    ]),
    "gallery",
    "main",
  );
  expect(items).not.toBeNull();
  expect(items).toHaveLength(4);
  const [first, , checked, danger] = items ?? [];
  expect(first).toMatchObject({ label: "Save image", icon: "download" });
  expect(checked).toMatchObject({ checked: true });
  expect(danger).toMatchObject({ danger: true, disabled: true });

  expect(first).toMatchObject({
    command: {
      type: "plugin-action",
      pluginId: "gallery",
      tileId: "main",
      action: "download",
      value: "https://x/1.png",
    },
  });
});

test("a plugin spec carries exactly one submenu level", () => {
  const items = parseContextItems(
    JSON.stringify([
      {
        label: "Sort by",
        icon: "list",
        items: [
          { action: "sort", value: "name", label: "Name", checked: true },
          { action: "sort", value: "date", label: "Date", checked: false },
          // A nested submenu is not representable: no action, so it drops.
          { label: "Deeper", items: [{ action: "x", label: "X" }] },
        ],
      },
    ]),
    "gallery",
    "main",
  );
  const submenu = items?.[0];
  expect(submenu !== undefined && isSubmenu(submenu)).toBe(true);
  if (submenu === undefined || !isSubmenu(submenu)) return;
  expect(submenu.items.map((item) => item.label)).toEqual(["Name", "Date"]);
  expect(submenu.items[0]?.checked).toBe(true);
  expect(submenu.items[1]?.checked).toBe(false);
});

test("broken plugin specs yield no menu at all", () => {
  expect(parseContextItems("{not json", "p", "w")).toBeNull();
  expect(parseContextItems('{"action":"a"}', "p", "w")).toBeNull();
  expect(parseContextItems("[]", "p", "w")).toBeNull();
  // Only separators is not a menu.
  expect(parseContextItems('[{"separator":true}]', "p", "w")).toBeNull();
  // An oversized attribute is refused unparsed.
  const huge = JSON.stringify([
    { action: "a", label: "x".repeat(CONTEXT_MENU_MAX_SPEC_CHARS) },
  ]);
  expect(parseContextItems(huge, "p", "w")).toBeNull();
});

test("individual broken entries drop, the rest of the menu survives", () => {
  const items = parseContextItems(
    JSON.stringify([
      { label: "no action" },
      { action: "a" },
      "nonsense",
      { action: "b", label: "Fine", icon: "no-such-icon" },
      {
        action: "c",
        label: "Too big",
        value: "v".repeat(CONTEXT_MENU_MAX_VALUE_CHARS + 1),
      },
    ]),
    "gallery",
    "main",
  );
  expect(items).toHaveLength(1);
  // Unknown icon names are dropped rather than rendered as a stray fallback.
  expect(items?.[0]).toMatchObject({ label: "Fine" });
  expect(items?.[0]).not.toHaveProperty("icon");
});

test("plugin menus are capped in item count and label length", () => {
  const many = Array.from({ length: CONTEXT_MENU_MAX_ITEMS + 5 }, (_, i) => ({
    action: "a",
    label: `Item ${String(i)}`,
  }));
  expect(parseContextItems(JSON.stringify(many), "p", "w")).toHaveLength(
    CONTEXT_MENU_MAX_ITEMS,
  );

  const long = parseContextItems(
    JSON.stringify([{ action: "a", label: "L".repeat(200) }]),
    "p",
    "w",
  );
  const label = long?.[0];
  expect(label !== undefined && "label" in label ? label.label.length : 0).toBe(
    CONTEXT_MENU_MAX_LABEL_CHARS,
  );
});

function markup(html: string): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = html;
  document.body.replaceChildren(host);
  return host;
}

function query(root: ParentNode, selector: string): Element {
  const found = root.querySelector(selector);
  if (found === null) throw new Error(`fixture is missing ${selector}`);
  return found;
}

/** The composed path an event on `element` would carry, innermost first. */
function pathOf(element: Element): EventTarget[] {
  const path: EventTarget[] = [];
  let node: Node | null = element;
  while (node !== null) {
    path.push(node);
    node = node.parentNode ?? (node instanceof ShadowRoot ? node.host : null);
  }
  return path;
}

test("a right-click resolves the innermost target first, then its fallbacks", () => {
  const host = markup(
    '<div data-bar-root><button data-tile-id="plugin:clock:clock"><span id="inner">x</span></button></div>',
  );
  const targets = resolveContextTargets(pathOf(query(host, "#inner")));
  expect(targets.map((target: ContextTarget) => target.kind)).toEqual([
    "tile",
    "bar",
  ]);
});

test("plugin markup resolves through the shadow host, tile menu as fallback", () => {
  const host = markup(
    '<div data-bar-root><button data-tile-id="plugin:demo:gallery">' +
      '<div id="shadow" data-plugin-id="demo" data-plugin-tile="gallery" ' +
      'data-tile-id="plugin:demo:gallery"></div></button></div>',
  );
  const root = query(host, "#shadow").attachShadow({ mode: "open" });
  root.innerHTML =
    '<img id="pic" data-context-items=\'[{"action":"save","label":"Save"}]\'>';

  const targets = resolveContextTargets(pathOf(query(root, "#pic")));
  expect(targets[0]).toEqual({
    kind: "plugin",
    pluginId: "demo",
    tileId: "gallery",
    spec: '[{"action":"save","label":"Save"}]',
  });
  // The tile stays in the list, so a broken plugin spec falls back to it.
  expect(targets.map((target) => target.kind)).toContain("tile");
});

test("plugin data attributes cannot spoof shell-owned context targets", () => {
  const host = markup(
    '<div data-bar-root><button data-tile-id="plugin:demo:gallery">' +
      '<div id="shadow" data-plugin-id="demo" data-plugin-tile="gallery" ' +
      'data-tile-id="plugin:demo:gallery"></div></button></div>',
  );
  const root = query(host, "#shadow").attachShadow({ mode: "open" });
  root.innerHTML =
    '<div id="spoof" data-context-items=\'[{"action":"save","label":"Save"}]\' ' +
    'data-plugin-id="weather" data-plugin-tile="main" ' +
    'data-tile-id="plugin:weather:main" data-shortcut-id="firefox" data-bar-root></div>';

  const targets = resolveContextTargets(pathOf(query(root, "#spoof")));
  expect(targets[0]).toEqual({
    kind: "plugin",
    pluginId: "demo",
    tileId: "gallery",
    spec: '[{"action":"save","label":"Save"}]',
  });
  expect(targets).not.toContainEqual({
    kind: "tile",
    id: "plugin:weather:main",
  });
  expect(targets).not.toContainEqual({ kind: "shortcut", id: "firefox" });
});

test("text fields keep the native menu", () => {
  const host = markup('<div data-bar-root><input id="field"></div>');
  expect(isEditableTarget(pathOf(query(host, "#field")))).toBe(true);
  expect(isEditableTarget(pathOf(host))).toBe(false);
});
