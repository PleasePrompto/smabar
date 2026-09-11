/**
 * The shell-owned context menus. Pure builders return items for the
 * ContextMenuLayer, using the same model as plugin-declared menus.
 */

import { t } from "../../i18n/t";
import type { ResolvedShortcut } from "../../store/bar";
import { pluginOfTile } from "../settings/pluginListModel";
import type { ContextMenuItem } from "./model";

/** Menu of one pinned shortcut; separator pins are removable but not runnable. */
export function shortcutMenu(shortcut: ResolvedShortcut): ContextMenuItem[] {
  const remove: ContextMenuItem = {
    id: "shortcut.remove",
    label: t(
      shortcut.separator ? "menu.separator.remove" : "menu.shortcut.remove",
    ),
    icon: "trash-2",
    danger: true,
    command: { type: "remove-shortcut", id: shortcut.id },
  };
  if (shortcut.separator) return [remove];
  return [
    {
      id: "shortcut.launch",
      label: t("menu.shortcut.launch"),
      icon: "external-link",
      command: { type: "launch-shortcut", id: shortcut.id },
    },
    { id: "shortcut.sep", separator: true },
    remove,
  ];
}

/**
 * The two PLUGIN-level actions, for a tile that has a plugin behind it.
 *
 * Deleting is irreversible, so it sits behind the one submenu level the menu
 * model allows — that is the confirmation, in place, instead of a dialog the
 * bar would have to open behind itself.
 */
function pluginItems(pluginId: string): ContextMenuItem[] {
  return [
    { id: "tile.plugin.sep", separator: true },
    {
      id: "tile.plugin.deactivate",
      label: t("menu.plugin.deactivate"),
      icon: "power",
      command: { type: "toggle-plugin", id: pluginId },
    },
    {
      id: "tile.plugin.delete",
      label: t("menu.plugin.delete"),
      icon: "trash-2",
      items: [
        {
          id: "tile.plugin.delete.confirm",
          label: t("menu.plugin.deleteConfirm"),
          icon: "trash-2",
          danger: true,
          command: { type: "remove-plugin", id: pluginId },
        },
      ],
    },
  ];
}

/** Menu of one registered plugin tile. */
export function tileMenu(tileId: string): ContextMenuItem[] {
  const pluginId = pluginOfTile(tileId);
  return [
    {
      id: "tile.hide",
      label: t("menu.plugin.hide"),
      icon: "eye-off",
      command: { type: "toggle-tile", id: tileId },
    },
    ...pluginItems(pluginId),
    { id: "tile.sep", separator: true },
    {
      id: "tile.settings",
      label: t("menu.plugin.settings"),
      icon: "settings",
      command: { type: "open-settings", group: "plugins" },
    },
  ];
}

/** Menu of the empty bar surface. */
export function barMenu(): ContextMenuItem[] {
  return [
    {
      id: "bar.settings",
      label: t("menu.bar.settings"),
      icon: "settings",
      command: { type: "open-settings" },
    },
  ];
}
