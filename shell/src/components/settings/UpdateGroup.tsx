import type { ReactNode } from "react";

import { t } from "../../i18n/t";
import { requestUpdate } from "../../ipc/updateSync";
import { reportError } from "../../ipc/log";
import { useSmabar, type UpdateStatus } from "../../store/bar";
import { SettingGroup, SettingRow } from "./controls";
import { formatDate, formatMebibytes } from "./storeModel";

function renderDetail(
  status: UpdateStatus,
  language: string,
): ReactNode | undefined {
  switch (status.state) {
    case "available":
      return (
        <div className="flex flex-col gap-1" aria-live="polite">
          <span className="sb-ok">
            {t("settings.update.available").replace(
              "{version}",
              status.version,
            )}
          </span>
          {status.date !== null && (
            <small className="sb-faint">
              {formatDate(status.date, language)}
            </small>
          )}
          {status.notes !== null && status.notes !== "" && (
            <p className="sb-faint whitespace-pre-line">{status.notes}</p>
          )}
          {status.installer !== null && (
            <div className="flex flex-col gap-1">
              <small className="sb-faint">
                {t(
                  status.installer === "app"
                    ? "settings.update.installDescriptionApp"
                    : "settings.update.installDescriptionSystem",
                )}
              </small>
              <button
                type="button"
                className="sb-btn sb-btn-primary self-start"
                onClick={() => {
                  void requestUpdate({
                    action: "install",
                    version: status.version,
                  }).catch(reportError);
                }}
              >
                {t("settings.update.install")}
              </button>
            </div>
          )}
        </div>
      );
    case "downloading":
      return (
        <span className="flex flex-col gap-1" aria-live="polite">
          <progress
            className="sb-progress"
            value={status.total === null ? undefined : status.received}
            max={status.total ?? undefined}
            aria-label={t("settings.update.downloading")}
          />
          <small className="sb-faint">
            {t("settings.update.downloading")}{" "}
            {formatMebibytes(status.received, language)}
            {status.total !== null &&
              ` / ${formatMebibytes(status.total, language)}`}
          </small>
        </span>
      );
    case "installing":
      return (
        <span className="sb-faint" aria-live="polite">
          {t("settings.update.installing")}
        </span>
      );
    case "handedOff":
      return (
        <span
          className={status.opened ? "sb-ok" : "sb-faint"}
          aria-live="polite"
        >
          {t(
            status.opened
              ? "settings.update.handedOff"
              : "settings.update.savedOnly",
          ).replace("{path}", status.path)}
        </span>
      );
    case "current":
      return (
        <span className="sb-faint" aria-live="polite">
          {t("settings.update.current")}
        </span>
      );
    case "failed":
      return (
        <span
          className="settings-font-error"
          role="alert"
          title={status.message}
        >
          {t(
            status.phase === "install"
              ? "settings.update.installFailed"
              : "settings.update.failed",
          )}
        </span>
      );
    case "idle":
    case "checking":
      return undefined;
  }
}

/**
 * Application updates (ADR 0009): the last check's outcome, a manual check
 * and — where this build can apply a release — the install itself.
 */
export function UpdateGroup() {
  const status = useSmabar((state) => state.updateStatus);
  const offer = useSmabar((state) => state.updateOffer);
  const language = useSmabar((state) => state.language);
  const checking = status.state === "checking";
  const busy =
    checking || status.state === "downloading" || status.state === "installing";

  return (
    <SettingGroup title={t("settings.update.title")} updateKey="app">
      <SettingRow
        label={t("settings.update.title")}
        description={t("settings.update.description")}
        control={
          <button
            type="button"
            className="sb-btn sb-btn-ghost"
            disabled={busy}
            onClick={() => {
              void requestUpdate({ action: "check" }).catch(reportError);
            }}
          >
            {checking
              ? t("settings.update.checking")
              : t("settings.update.check")}
          </button>
        }
      >
        {renderDetail(status, language)}
        {offer !== null &&
          status.state === "failed" &&
          renderDetail({ state: "available", ...offer }, language)}
      </SettingRow>
    </SettingGroup>
  );
}
