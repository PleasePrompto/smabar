import { Blocks, ChevronRight } from "lucide-react";

import { t } from "../../i18n/t";
import { useSmabar, type InstalledPlugin } from "../../store/bar";
import { PluginIcon } from "../../plugins/PluginIcon";
import { AudioLevelControls } from "./AudioGroup";
import { SettingRow } from "./controls";
import { PluginSettings } from "./PluginSettings";
import { OriginBadge } from "./StoreBadge";
import { StoreUpdateLink } from "./UpdateBadge";
import { useAudioSettings, type AudioSettings } from "./useAudioSettings";
import type { PluginManagement } from "./usePluginManagement";
import { PluginActions } from "./PluginActions";

export function PluginCards({ management }: { management: PluginManagement }) {
  const audio = useAudioSettings();
  return (
    <>
      <div className="settings-intro">
        <h3 id="plugin-settings-title" tabIndex={-1}>
          {t("settings.plugins.pluginSettings")}
        </h3>
        <p>{t("settings.plugins.pluginSettingsDescription")}</p>
      </div>
      {management.installed?.map((plugin) => (
        <TileCard
          key={plugin.id}
          plugin={plugin}
          management={management}
          audio={audio}
        />
      ))}
    </>
  );
}

function TileCard({
  plugin,
  management,
  audio,
}: {
  plugin: InstalledPlugin;
  management: PluginManagement;
  audio: AudioSettings;
}) {
  const deactivated = useSmabar((state) =>
    state.pluginsDeactivated.includes(plugin.id),
  );
  const hidden = useSmabar((state) => state.pluginsHidden);
  const name = t(plugin.name ?? plugin.id);
  const tiles = plugin.tiles.map((tile) => ({
    id: `plugin:${plugin.id}:${tile.id}`,
    name: tile.name,
  }));
  const first = plugin.tiles[0];
  const status = deactivated ? "deactivated" : plugin.status;
  const allHidden =
    tiles.length > 0 && tiles.every((tile) => hidden.includes(tile.id));
  const path = `audio.plugins.${plugin.id}`;

  return (
    <section
      className="settings-block settings-plugin-card"
      aria-label={name}
      data-update-key={`plugin:${plugin.id}`}
    >
      <div className="settings-box">
        <div className="settings-plugin-header">
          <span className="settings-plugin-icon" aria-hidden="true">
            <PluginIcon
              pluginId={plugin.id}
              tileId={first?.id ?? ""}
              svg={first?.iconSvg}
              dataUrl={plugin.iconDataUrl}
              style={{ fontSize: "2.5rem" }}
              fallback={<Blocks size="0.6em" />}
            />
          </span>
          <div className="settings-plugin-identity">
            <h3 className="settings-block-title">{name}</h3>
            <div className="settings-plugin-facts">
              <OriginBadge provenance={plugin} />
              <StoreUpdateLink kind="plugin" id={plugin.id} />
              {plugin.origin !== "community" && plugin.version !== null && (
                <span className="sb-mono sb-dim">v{plugin.version}</span>
              )}
              <span
                className={`sb-badge${status === "running" ? " sb-badge-success" : status === "failed" ? " sb-badge-danger" : ""}`}
              >
                {t(`settings.plugins.status.${status}`)}
              </span>
              {allHidden && (
                <span className="sb-badge">
                  {t("settings.plugins.stateHidden")}
                </span>
              )}
            </div>
          </div>
        </div>
        {plugin.description && (
          <p className="settings-plugin-description">{plugin.description}</p>
        )}
        {status === "failed" && (
          <p className="settings-plugin-error">
            {t("settings.plugins.startFailed")}
          </p>
        )}
        {plugin.blocked !== null && (
          <p className="settings-plugin-error">
            {t("settings.plugins.originBlocked").replace(
              "{reason}",
              plugin.blocked,
            )}
          </p>
        )}
        <div className="settings-plugin-toolbar">
          {tiles.length > 1 && (
            <p className="settings-help">
              {t("settings.plugins.sharedSettings").replace(
                "{count}",
                String(tiles.length),
              )}
            </p>
          )}
          <PluginActions
            pluginId={plugin.id}
            name={name}
            tiles={tiles}
            tileCount={tiles.length}
            management={management}
          />
        </div>
        <details className="settings-plugin-details">
          <summary>
            <ChevronRight size="1em" aria-hidden="true" />
            {t("settings.plugins.configure")}
          </summary>
          <PluginSettings pluginId={plugin.id} schema={plugin.settingsSchema} />
          <SettingRow
            label={t("settings.audio.plugin")}
            description={t("settings.audio.pluginDescription")}
            wide
          >
            <AudioLevelControls
              audio={audio}
              path={path}
              level={
                audio.config === null
                  ? null
                  : (audio.config.plugins[plugin.id] ?? {
                      volume: 100,
                      muted: false,
                    })
              }
            />
            {audio.config?.muted === true && (
              <p className="settings-help">{t("settings.audio.masterMuted")}</p>
            )}
          </SettingRow>
        </details>
      </div>
    </section>
  );
}
