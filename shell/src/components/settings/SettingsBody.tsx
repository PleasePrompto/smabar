import { useAudioSettings } from "./useAudioSettings";
import { t } from "../../i18n/t";
import { BarTab } from "./BarTab";
import { DesignTab } from "./DesignTab";
import { ShortcutsTab } from "./ShortcutsTab";
import { PluginsTab } from "./PluginsTab";
import { PluginCard } from "./PluginCards";
import { SystemTab } from "./SystemTab";
import { SettingsSection } from "./controls";
import type { PluginManagement } from "./usePluginManagement";

export function SettingsBody({
  page,
  management,
}: {
  page: string;
  management: PluginManagement;
}) {
  switch (page) {
    case "bar/layout":
      return <BarTab />;
    case "bar/behavior":
      return <BarTab page="behavior" />;
    case "bar/themes":
      return <DesignTab />;
    case "bar/colors":
      return <DesignTab page="colors" />;
    case "bar/appearance":
      return <DesignTab page="appearance" />;
    case "shortcuts":
      return <ShortcutsTab />;
    case "plugins":
      return <PluginsTab management={management} />;
    case "system/general":
      return <SystemTab />;
    case "system/audio":
      return <SystemTab page="audio" />;
    case "system/advanced":
      return <SystemTab page="advanced" />;
    case "system/about":
      return <SystemTab page="about" />;
    case "system/legal":
      return <SystemTab page="legal" />;
    default:
      return <PluginDetail page={page} management={management} />;
  }
}

function PluginDetail({
  page,
  management,
}: {
  page: string;
  management: PluginManagement;
}) {
  const audio = useAudioSettings();
  const plugin = management.installed?.find(
    (entry) => page === `plugins/detail/${entry.id}`,
  );
  return (
    <SettingsSection
      title={
        plugin === undefined
          ? t("settings.plugins.installed")
          : t(plugin.name ?? plugin.id)
      }
    >
      {plugin === undefined ? (
        <p role="status">
          {t(
            management.installed === null
              ? "settings.plugins.loading"
              : "settings.plugins.missing",
          )}
        </p>
      ) : (
        <PluginCard plugin={plugin} management={management} audio={audio} />
      )}
    </SettingsSection>
  );
}
