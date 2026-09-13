import { Download, X } from "lucide-react";
import { t } from "../i18n/t";
import { reportError } from "../ipc/log";
import { requestUpdate } from "../ipc/updateSync";
import { useSmabar } from "../store/bar";

/** Host-owned, sticky, and independent of the plugin popup queue and mute switch. */
export function AppUpdateNotification() {
  const offer = useSmabar((state) => state.updateOffer);
  const checking = useSmabar(
    (state) => state.updateStatus.state === "checking",
  );
  const failed = useSmabar(
    (state) =>
      state.updateStatus.state === "failed" &&
      state.updateStatus.phase === "install",
  );
  if (offer === null) return null;
  return (
    <div
      className="sb-root surface-flyout app-update-notification"
      data-input-region
      data-capture="popup"
      role="status"
      aria-live="polite"
    >
      <button
        type="button"
        className="notification-close sb-btn sb-btn-ghost sb-btn-icon"
        aria-label={t("popup.dismiss")}
        title={t("popup.dismiss")}
        onClick={() => {
          void requestUpdate({
            action: "dismiss",
            version: offer.version,
          }).catch(reportError);
        }}
      >
        <X size="1.125rem" />
      </button>
      <strong>
        {t("settings.update.notification").replace("{version}", offer.version)}
      </strong>
      {offer.installer !== null && (
        <p>
          {t(
            failed
              ? "settings.update.installFailed"
              : offer.installer === "app"
                ? "settings.update.installDescriptionApp"
                : "settings.update.installDescriptionSystem",
          )}
        </p>
      )}
      <button
        type="button"
        className="sb-btn sb-btn-primary"
        disabled={checking}
        onClick={() => {
          void requestUpdate(
            offer.installer === null
              ? { action: "details" }
              : { action: "install", version: offer.version },
          ).catch(reportError);
        }}
      >
        <Download size="1em" aria-hidden="true" />
        {t(
          offer.installer === null
            ? "settings.store.details"
            : "settings.update.now",
        )}
      </button>
    </div>
  );
}
