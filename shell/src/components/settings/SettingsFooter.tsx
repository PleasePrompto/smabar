import { ArrowUpCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import { hasAppUpdate } from "../../ipc/updateSync";
import { useSmabar } from "../../store/bar";
import { LanguageSelect } from "./LanguageChoice";
import { useAppVersion } from "./useAppVersion";

export function SettingsFooter() {
  const version = useAppVersion();
  const language = useSmabar((state) => state.language);
  const update = useSmabar(hasAppUpdate);
  const [languages, setLanguages] = useState<readonly string[] | null>(null);
  useEffect(() => {
    let cancelled = false;
    call<{ languages: string[] }>("get_system_settings")
      .then((system) => {
        if (!cancelled) setLanguages(system.languages);
      })
      .catch(reportError);
    return () => {
      cancelled = true;
    };
  }, []);
  return (
    <footer className="settings-sidebar-footer">
      <div className="settings-footer-identity">
        <span
          className="settings-info-logo"
          role="img"
          aria-label={t("settings.info.logoLabel")}
        />
        <button
          type="button"
          className="settings-version-chip"
          data-update={update || undefined}
          title={t(update ? "settings.update.badge" : "settings.page.about")}
          onClick={() => {
            useSmabar.getState().setSettingsGroup("system/about");
          }}
        >
          <span translate="no">
            {version === "dev" ? version : `v${version}`}
          </span>
          {update && (
            <>
              <ArrowUpCircle size="1em" aria-hidden="true" />
              <span className="sb-sr-only">{t("settings.update.badge")}</span>
            </>
          )}
        </button>
      </div>
      <LanguageSelect languages={languages ?? [language]} />
    </footer>
  );
}
