import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import { openSettings } from "../../ipc/surface";
import { useSmabar } from "../../store/bar";
import { sendPluginAction } from "../../plugins/PluginContent";
import { toggleDisabled } from "../settings/model";
import { setConfig } from "../settings/persist";
import type { ContextMenuCommand } from "./model";

export function dispatchContextCommand(command: ContextMenuCommand): void {
  switch (command.type) {
    case "plugin-action":
      sendPluginAction(
        command.pluginId,
        command.tileId,
        command.action,
        command.value,
      );
      break;
    case "launch-shortcut":
      void call("launch_shortcut", { id: command.id }).catch(reportError);
      break;
    case "remove-shortcut":
      void call("unpin_shortcut", { id: command.id }).catch(reportError);
      break;
    case "toggle-tile":
      setConfig(
        "pluginsHidden",
        toggleDisabled(useSmabar.getState().pluginsHidden, command.id),
      );
      break;
    case "toggle-plugin":
      setConfig(
        "pluginsDeactivated",
        toggleDisabled(useSmabar.getState().pluginsDeactivated, command.id),
      );
      break;
    case "remove-plugin":
      void call("remove_plugin", { pluginId: command.id }).catch(reportError);
      break;
    case "open-settings":
      void openSettings(command.group).catch(reportError);
      break;
  }
}
