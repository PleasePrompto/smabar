import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import { useSmabar, type RuntimeStatusInfo } from "../../store/bar";
import { SettingGroup, SettingRow } from "./controls";

function failureText(runtime: RuntimeStatusInfo): string {
  if (runtime.kind === "offline") return t("settings.system.runtimeOffline");
  if (runtime.kind === "uvMissing") {
    return t("settings.system.runtimeUvMissing");
  }
  return t("settings.system.runtimeFailed");
}

/**
 * Managed-Python-runtime status: indeterminate progress while uv installs
 * (with its last output line as detail), a ready mark, or the failure +
 * retry block (same pattern as the font install error). `null` means the
 * bridge has not fetched a snapshot yet — rendered like `absent`.
 */
export function RuntimeGroup() {
  const runtime = useSmabar((state) => state.runtimeStatus);

  return (
    <SettingGroup title={t("settings.system.runtime")}>
      <SettingRow
        label={t("settings.system.runtime")}
        description={t("settings.system.runtimeDescription")}
      >
        {runtime === null || runtime.state === "absent" ? (
          <span className="sb-faint">{t("settings.system.runtimeAbsent")}</span>
        ) : runtime.state === "installing" ? (
          <span className="flex flex-col gap-1" aria-live="polite">
            <progress
              className="sb-progress"
              aria-label={t("settings.system.runtimeInstalling")}
            />
            {typeof runtime.detail === "string" && runtime.detail !== "" && (
              <small className="sb-faint">{runtime.detail}</small>
            )}
          </span>
        ) : runtime.state === "ready" ? (
          <span className="sb-ok">{t("settings.system.runtimeReady")}</span>
        ) : (
          <div className="settings-font-error" role="alert">
            <span title={runtime.message ?? undefined}>
              {failureText(runtime)}
            </span>
            <button
              type="button"
              className="sb-btn sb-btn-ghost"
              onClick={() => {
                void call("retry_provisioning").catch(reportError);
              }}
            >
              {t("settings.system.runtimeRetry")}
            </button>
          </div>
        )}
      </SettingRow>
    </SettingGroup>
  );
}
