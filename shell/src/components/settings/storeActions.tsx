import { t } from "../../i18n/t";
import type { StoreEntry, StoreProgress } from "../../ipc/store";
import {
  formatMebibytes,
  type StoreAction,
  type StoreState,
} from "./storeModel";

export function actionLabel(action: StoreAction, entry: StoreEntry): string {
  switch (action) {
    case "install":
      return t("settings.store.install").replace("{version}", entry.version);
    case "update":
      return t("settings.store.update").replace(
        "{version}",
        entry.update?.toVersion ?? entry.version,
      );
    case "uninstall":
      return t("settings.store.uninstall");
  }
}

/**
 * The inline question. It names the version and the repository, and every
 * install wording says that smabar has not reviewed the code — the one
 * sentence the trust model requires the user to have read.
 */
export function confirmQuestion(
  action: StoreAction,
  entry: StoreEntry,
  state: StoreState,
): string {
  const fill = (key: string) =>
    t(key)
      .replace("{name}", entry.name)
      .replace("{version}", entry.version)
      .replace("{from}", entry.installed?.version ?? "")
      .replace("{to}", entry.update?.toVersion ?? entry.version)
      .replace("{repo}", entry.repo.nameWithOwner);
  switch (action) {
    case "install":
      return fill("settings.store.confirmInstall");
    case "update":
      if (state === "modified") return fill("settings.store.confirmModified");
      if (state === "contentChanged") {
        return fill("settings.store.confirmContentChanged");
      }
      return fill("settings.store.confirmUpdate");
    case "uninstall":
      return fill(
        entry.kind === "theme"
          ? "settings.store.confirmUninstallTheme"
          : "settings.store.confirmUninstall",
      );
  }
}

function phaseLabel(progress: StoreProgress): string {
  switch (progress.phase) {
    case "downloading":
      return t("settings.store.phaseDownloading");
    case "verifying":
      return t("settings.store.phaseVerifying");
    case "installing":
      return t("settings.store.phaseInstalling");
    case "starting":
      return t("settings.store.phaseStarting");
    case "done":
      return t("settings.store.phaseDone");
  }
}

export function Progress({
  progress,
  language,
}: {
  progress: StoreProgress;
  language: string;
}) {
  const label = phaseLabel(progress);
  const downloading = progress.phase === "downloading";
  return (
    <div className="settings-store-progress" aria-live="polite">
      {progress.phase !== "done" && (
        <progress
          className="sb-progress"
          value={
            downloading && progress.total !== null
              ? progress.received
              : undefined
          }
          max={downloading ? (progress.total ?? undefined) : undefined}
          aria-label={label}
        />
      )}
      <small className="sb-faint">
        {label}
        {downloading && ` ${formatMebibytes(progress.received, language)}`}
        {downloading &&
          progress.total !== null &&
          ` / ${formatMebibytes(progress.total, language)}`}
      </small>
    </div>
  );
}

/**
 * The buttons a listing offers, install or update first. Each one only
 * asks; the caller runs the confirmed action.
 */
export function ActionButtons({
  entry,
  offered,
  disabled,
  onAsk,
  buttonRef,
}: {
  entry: StoreEntry;
  offered: readonly StoreAction[];
  disabled: boolean;
  onAsk: (action: StoreAction) => void;
  /** Lets the caller return focus to the button that asked. */
  buttonRef?: (action: StoreAction, node: HTMLButtonElement | null) => void;
}) {
  return (
    <>
      {offered.map((action) => (
        <button
          key={action}
          ref={(node) => {
            buttonRef?.(action, node);
          }}
          type="button"
          data-store-action={action}
          className={
            action === "uninstall"
              ? "sb-btn sb-btn-ghost"
              : "sb-btn sb-btn-primary"
          }
          disabled={disabled}
          onClick={() => {
            onAsk(action);
          }}
        >
          {actionLabel(action, entry)}
        </button>
      ))}
    </>
  );
}
