import { Store } from "lucide-react";
import { t } from "../../i18n/t";
import { useSmabar } from "../../store/bar";
import { SettingGroup, SettingRow, SettingsSection } from "./controls";
import { RuntimeGroup } from "./RuntimeGroup";
import type { PluginManagement } from "./usePluginManagement";
import { PluginList } from "./PluginList";
import { PluginImport } from "./PluginImport";
import { setConfigsSequentially } from "./persist";

export function PluginsTab({ management }: { management: PluginManagement }) {
  const runtime = useSmabar((state) => state.runtimeStatus);
  const setGroup = useSmabar((state) => state.setSettingsGroup);
  return (
    <SettingsSection
      title={t("settings.group.plugins")}
      onReset={() =>
        setConfigsSequentially([{ path: "pluginOrder", value: [] }])
      }
    >
      <PluginImport onInstalled={management.refresh} />
      {runtime !== null &&
        (runtime.state === "installing" || runtime.state === "failed") && (
          // Python plugins wait for the runtime: its download or failure
          // (with Retry) belongs where the waiting cards are.
          <RuntimeGroup />
        )}
      <SettingGroup title={t("settings.plugins.installed")} updateKey="plugin">
        <SettingRow
          label={t("settings.plugins.installed")}
          description={t("settings.plugins.listDescription")}
          wide
        >
          <PluginList management={management} />
          {management.loadFailed ? (
            <div className="settings-plugin-error" role="alert">
              <span>{t("settings.plugins.loadFailed")}</span>
              <button
                type="button"
                className="sb-btn sb-btn-ghost"
                onClick={() => {
                  void management.refresh();
                }}
              >
                {t("settings.plugins.retry")}
              </button>
            </div>
          ) : (
            management.installed === null && (
              <p className="settings-help">{t("settings.plugins.loading")}</p>
            )
          )}
        </SettingRow>
        <SettingRow
          label={t("settings.store.title")}
          description={t("settings.plugins.storeTeaser")}
          control={
            <button
              type="button"
              className="sb-btn"
              onClick={() => {
                setGroup("plugins/store");
              }}
            >
              <Store size="1em" aria-hidden="true" />
              {t("settings.plugins.storeTeaserAction")}
            </button>
          }
        />
      </SettingGroup>
    </SettingsSection>
  );
}
