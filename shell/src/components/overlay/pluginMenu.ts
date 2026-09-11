/**
 * Parser for the plugin-declared context menu.
 *
 * Plugins declare menus DECLARATIVELY on any element of their (sanitized)
 * HTML:
 *
 *   <img data-context-items='[{"action":"download","value":"…",
 *        "label":"Download image","icon":"download"}]' …>
 *
 * `data-*` survives the sanitizer untouched and a selected item travels the
 * EXISTING `plugin_action` path — no new protocol, no SDK change. Labels
 * come from the plugin (it owns its own i18n) and are inserted as TEXT.
 *
 * Everything here is defensive: the attribute is untrusted plugin output.
 * A broken spec yields `null` (no menu at all, the caller falls back to the
 * tile menu); individual broken entries are dropped.
 *
 * The attribute name and every limit are mirrored in ui-kit/contract.json
 * (served to agents by the MCP `ui_kit` tool); an anti-drift test keeps them
 * equal.
 */

import { ICONS } from "../../plugins/icons";
import type {
  ContextMenuAction,
  ContextMenuItem,
  ContextMenuSubmenu,
} from "./model";

/** Attribute a plugin declares its context menu on. */
export const CONTEXT_ITEMS_ATTR = "data-context-items";
/** Maximum entries per level (top level and submenu alike). */
export const CONTEXT_MENU_MAX_ITEMS = 12;
/** Longer labels are truncated with an ellipsis. */
export const CONTEXT_MENU_MAX_LABEL_CHARS = 64;
/** An item with a longer action name is dropped. */
export const CONTEXT_MENU_MAX_ACTION_CHARS = 64;
/** An item with a longer value payload is dropped. */
export const CONTEXT_MENU_MAX_VALUE_CHARS = 512;
/** A longer attribute is refused unparsed. */
export const CONTEXT_MENU_MAX_SPEC_CHARS = 4096;

type Raw = Record<string, unknown>;

const isRaw = (value: unknown): value is Raw =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** Trimmed, length-capped display text; null when unusable. */
function label(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  if (trimmed === "") return null;
  return trimmed.length > CONTEXT_MENU_MAX_LABEL_CHARS
    ? `${trimmed.slice(0, CONTEXT_MENU_MAX_LABEL_CHARS - 1)}…`
    : trimmed;
}

/** Registry-validated icon name; unknown names render no icon at all. */
function icon(value: unknown): string | undefined {
  return typeof value === "string" && value in ICONS ? value : undefined;
}

function parseAction(
  raw: Raw,
  id: string,
  pluginId: string,
  tileId: string,
): ContextMenuAction | null {
  const text = label(raw.label);
  if (text === null) return null;
  if (
    typeof raw.action !== "string" ||
    raw.action === "" ||
    raw.action.length > CONTEXT_MENU_MAX_ACTION_CHARS
  ) {
    return null;
  }
  const action = raw.action;
  let value: string | undefined;
  if (raw.value !== undefined) {
    if (
      typeof raw.value !== "string" ||
      raw.value.length > CONTEXT_MENU_MAX_VALUE_CHARS
    ) {
      return null;
    }
    value = raw.value;
  }
  return {
    id,
    label: text,
    ...(icon(raw.icon) !== undefined && { icon: icon(raw.icon) }),
    ...(raw.danger === true && { danger: true }),
    ...(raw.disabled === true && { disabled: true }),
    ...(typeof raw.checked === "boolean" && { checked: raw.checked }),
    command: {
      type: "plugin-action",
      pluginId,
      tileId,
      action,
      ...(value !== undefined && { value }),
    },
  };
}

/** One submenu level: its children are leaves, nested `items` are ignored. */
function parseSubmenu(
  raw: Raw,
  children: unknown[],
  id: string,
  pluginId: string,
  tileId: string,
): ContextMenuSubmenu | null {
  const text = label(raw.label);
  if (text === null) return null;
  const items: ContextMenuAction[] = [];
  for (const child of children.slice(0, CONTEXT_MENU_MAX_ITEMS)) {
    if (!isRaw(child)) continue;
    const item = parseAction(
      child,
      `${id}:${String(items.length)}`,
      pluginId,
      tileId,
    );
    if (item !== null) items.push(item);
  }
  if (items.length === 0) return null;
  return {
    id,
    label: text,
    ...(icon(raw.icon) !== undefined && { icon: icon(raw.icon) }),
    ...(raw.disabled === true && { disabled: true }),
    items,
  };
}

/**
 * Turns a `data-context-items` attribute into menu items. Returns null when
 * the spec is unusable (not JSON, not an array, or nothing selectable left)
 * so the caller can fall back to the tile's own menu.
 */
export function parseContextItems(
  spec: string,
  pluginId: string,
  tileId: string,
): ContextMenuItem[] | null {
  if (spec.length > CONTEXT_MENU_MAX_SPEC_CHARS) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(spec);
  } catch {
    // Malformed JSON is a plugin bug, not a shell error — the caller logs it
    // with the plugin id attached so it shows up in that plugin's log.
    return null;
  }
  if (!Array.isArray(parsed)) return null;

  const items: ContextMenuItem[] = [];
  for (const entry of parsed.slice(0, CONTEXT_MENU_MAX_ITEMS)) {
    if (!isRaw(entry)) continue;
    const id = `plugin-item-${String(items.length)}`;
    if (entry.separator === true) {
      items.push({ id, separator: true });
      continue;
    }
    const item = Array.isArray(entry.items)
      ? parseSubmenu(entry, entry.items, id, pluginId, tileId)
      : parseAction(entry, id, pluginId, tileId);
    if (item !== null) items.push(item);
  }
  // A menu of nothing but separators is not a menu.
  return items.some((item) => !("separator" in item)) ? items : null;
}
