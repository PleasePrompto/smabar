import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState } from "react";

import { t } from "../../i18n/t";
import { reportError } from "../../ipc/log";
import { SettingGroup } from "./controls";

/** Product identity and runtime build information; intentionally no settings. */
export function AboutGroup() {
  // Cargo.toml is the one version source; only the Tauri window knows it.
  const [version, setVersion] = useState("dev");

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let cancelled = false;
    getVersion()
      .then((current) => {
        if (!cancelled) setVersion(current);
      })
      .catch(reportError);
    return () => {
      cancelled = true;
    };
  }, []);

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
