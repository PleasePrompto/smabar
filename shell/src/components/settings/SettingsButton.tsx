import { Settings } from "lucide-react";

import { t } from "../../i18n/t";
import { reportError } from "../../ipc/log";
import { toggleSettings } from "../../ipc/surface";
import { useSmabar } from "../../store/bar";
import { hasAppUpdate } from "../../ipc/updateSync";

/**
 * Discreet gear at the right end of the bar; toggles the settings panel.
 * Its dot is passive: an app release, or Community Plugins with a newer
 * version — the app release names the dot when both apply.
 */
export function SettingsButton() {
  const card = useSmabar((s) => s.appearance.tileChrome === "card");
  const updateAvailable = useSmabar(hasAppUpdate);
  const communityUpdates = useSmabar((s) => s.communityUpdates.length);
  const badge = updateAvailable
    ? t("settings.update.badge")
    : communityUpdates > 0
      ? t("settings.store.badge").replace("{count}", String(communityUpdates))
      : null;
  return (
    <button
      className={`${card ? "surface-tile surface-tile-hover" : ""} bar-hover-foreground text-dim relative flex shrink-0 items-center justify-center rounded-sb-s p-1.5`}
      data-tile-chrome={card ? "card" : "flat"}
      onClick={(e) => {
        e.stopPropagation();
        void toggleSettings().catch(reportError);
      }}
      aria-label={
        badge === null ? t("settings.open") : `${t("settings.open")} · ${badge}`
      }
      aria-haspopup="dialog"
    >
      <Settings size="1em" />
      {badge !== null && (
        <span
          className="absolute top-0.5 right-0.5 size-1.5 rounded-full bg-[color:var(--sb-warning)]"
          role="img"
          aria-label={badge}
        />
      )}
    </button>
  );
}
