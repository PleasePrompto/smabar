import { t } from "../../i18n/t";
import { SettingGroup, SettingRow, Switch } from "./controls";
import { NumberRow } from "./NumberRow";
import {
  useAudioSettings,
  type AudioLevel,
  type AudioSettings,
} from "./useAudioSettings";

export function AudioGroup() {
  const audio = useAudioSettings();
  return (
    <SettingGroup title={t("settings.audio.title")}>
      <SettingRow
        label={t("settings.audio.master")}
        description={t("settings.audio.description")}
        wide
      >
        <AudioLevelControls audio={audio} path="audio" level={audio.config} />
      </SettingRow>
      {audio.config !== null && (
        <SettingRow
          label={t("settings.audio.notifications")}
          control={
            <Switch
              label={t("settings.audio.notifications")}
              checked={audio.config.notificationSounds}
              disabled={audio.saving}
              onChange={(enabled) => {
                void audio.write("audio.notificationSounds", enabled);
              }}
            />
          }
        />
      )}
    </SettingGroup>
  );
}

export function AudioLevelControls({
  audio,
  path,
  level,
}: {
  audio: AudioSettings;
  path: string;
  level: AudioLevel | null;
}) {
  const failed =
    audio.failedPath === "audio" ||
    audio.failedPath?.startsWith(`${path}.`) === true;
  return (
    <div className="settings-audio-controls">
      {level !== null && (
        <>
          <label className="settings-audio-volume" htmlFor={`${path}-volume`}>
            <span>{t("settings.audio.volume")}</span>
            <NumberRow
              id={`${path}-volume`}
              label={t("settings.audio.volume")}
              value={level.volume}
              min={0}
              max={100}
              disabled={audio.saving}
              onChange={(volume) => {
                void audio.write(`${path}.volume`, volume);
              }}
            />
            <span className="sb-dim" aria-hidden="true">
              %
            </span>
          </label>
          <label className="settings-audio-mute">
            <Switch
              label={t("settings.audio.muted")}
              checked={level.muted}
              disabled={audio.saving}
              onChange={(muted) => {
                void audio.write(`${path}.muted`, muted);
              }}
            />
            {t("settings.audio.muted")}
          </label>
        </>
      )}
      {failed ? (
        <div className="settings-plugin-error" role="alert">
          <span>{t("settings.audio.failed")}</span>
          <button
            type="button"
            className="sb-btn sb-btn-ghost"
            disabled={audio.saving}
            onClick={() => {
              void audio.refresh();
            }}
          >
            {t("settings.plugins.retry")}
          </button>
        </div>
      ) : (
        level === null && (
          <p className="settings-help">{t("settings.audio.loading")}</p>
        )
      )}
    </div>
  );
}
