import { t } from "../../i18n/t";
import { SettingGroup } from "./controls";
import { useAppVersion } from "./useAppVersion";

/** Product identity and runtime build information; intentionally no settings. */
export function AboutGroup() {
  const version = useAppVersion();

  return (
    <SettingGroup title={t("settings.system.about")}>
      <div className="settings-info">
        <div className="settings-info-hero">
          <span
            className="settings-info-logo"
            role="img"
            aria-label={t("settings.info.logoLabel")}
          />
          <span
            className="settings-info-version"
            translate="no"
            aria-live="polite"
          >
            {t("settings.info.version").replace("{version}", version)}
          </span>
        </div>
        <div className="settings-info-card">
          <p>{t("settings.info.description")}</p>
          <p className="settings-info-principle">
            {t("settings.info.principle")}
          </p>
        </div>
      </div>
    </SettingGroup>
  );
}
