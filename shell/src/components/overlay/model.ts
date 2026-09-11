import type { FlyoutMode } from "../../store/bar";

/**
 * Item model of the ONE central context menu. Every area of the shell — and
 * every plugin — feeds this same shape into `openContextMenu`; there is no
 * second menu implementation anywhere.
 *
 * Exactly ONE submenu level is representable: a submenu's children are
 * leaves. Deeper nesting is deliberately impossible — the bar is a narrow
 * chrome surface glued to a screen edge, and a third level makes edge
 * clamping and keyboard navigation disproportionate.
 */

export type ContextMenuCommand =
  | {
      type: "plugin-action";
      pluginId: string;
      tileId: string;
      action: string;
      value?: string;
    }
  | { type: "launch-shortcut"; id: string }
  | { type: "remove-shortcut"; id: string }
  | { type: "toggle-tile"; id: string }
  | { type: "toggle-plugin"; id: string }
  | { type: "remove-plugin"; id: string }
  | { type: "open-settings"; group?: string };

/** A leaf entry: activating it dispatches one allowlisted command. */
export interface ContextMenuAction {
  id: string;
  /** Plain text — never markup. */
  label: string;
  /** Icon name from the shared registry (tiles/icons.ts). */
  icon?: string;
  /** Destructive styling (removing, deleting). */
  danger?: boolean;
  /** Rendered, but neither focusable nor activatable. */
  disabled?: boolean;
  /** Renders a check mark; the OWNER keeps the state, the menu only shows it. */
  checked?: boolean;
  command: ContextMenuCommand;
}

/** A hairline between two groups. */
export interface ContextMenuSeparator {
  id: string;
  separator: true;
}

/** A submenu; its children are leaves, so nesting stops right here. */
export interface ContextMenuSubmenu {
  id: string;
  label: string;
  icon?: string;
  disabled?: boolean;
  items: ContextMenuAction[];
}

export type ContextMenuItem =
  ContextMenuAction | ContextMenuSeparator | ContextMenuSubmenu;

export function isSeparator(
  item: ContextMenuItem,
): item is ContextMenuSeparator {
  return "separator" in item;
}

export function isSubmenu(item: ContextMenuItem): item is ContextMenuSubmenu {
  return "items" in item;
}

/** Whether an item can take focus and be activated. */
export function isEnabled(item: ContextMenuItem): boolean {
  return !isSeparator(item) && item.disabled !== true;
}

/**
 * Drops leading, trailing and repeated separators. Menus are assembled from
 * conditional parts (a separator pin has no "launch" entry, a plugin may
 * emit an empty group), and a dangling hairline looks broken.
 */
export function compactSeparators(
  items: readonly ContextMenuItem[],
): ContextMenuItem[] {
  const compact: ContextMenuItem[] = [];
  for (const item of items) {
    const last = compact.at(-1);
    if (isSeparator(item) && (last === undefined || isSeparator(last))) {
      continue;
    }
    compact.push(item);
  }
  const last = compact.at(-1);
  if (last !== undefined && isSeparator(last)) compact.pop();
  return compact;
}

/**
 * Index of the next focusable item `step` positions away, wrapping around
 * and skipping separators and disabled entries. `from` may sit outside the
 * list (seed -1 for "first", `items.length` for "last"). Returns -1 when no
 * item can be focused at all.
 */
export function nextEnabledIndex(
  items: readonly ContextMenuItem[],
  from: number,
  step: number,
): number {
  const count = items.length;
  if (count === 0) return -1;
  for (let hop = 1; hop <= count; hop++) {
    const index = (((from + hop * step) % count) + count) % count;
    const item = items[index];
    if (item !== undefined && isEnabled(item)) return index;
  }
  return -1;
}

/** Selects the pushed HTML for a hover preview or pinned flyout. */
export function flyoutContentFor(
  mode: FlyoutMode | null,
  hoverHtml: string | undefined,
  flyoutHtml: string | undefined,
): string | undefined {
  return mode === "peek"
    ? (hoverHtml ?? flyoutHtml)
    : (flyoutHtml ?? hoverHtml);
}
