import { PluginsTab as Overview } from "./PluginsTab";
import { usePluginManagement } from "./usePluginManagement";
import { PluginCard } from "./PluginCards";
import { PluginPresentation } from "./PluginPresentation";
import { useAudioSettings } from "./useAudioSettings";
export function PluginsTab() {
  const audio = useAudioSettings();
  const management = usePluginManagement();
  return (
    <div id="settings-content" tabIndex={0}>
      <Overview management={management} />
      <PluginPresentation />
      <PluginPresentation behavior />
      {management.installed?.map((plugin) => (
        <PluginCard
          key={plugin.id}
          plugin={plugin}
          management={management}
          audio={audio}
        />
      ))}
    </div>
  );
}
