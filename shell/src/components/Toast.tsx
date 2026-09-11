import { t } from "../i18n/t";
import { useSmabar } from "../store/bar";

/**
 * Transient hint near the bar edge (e.g. a rejected drop). The store's
 * `notice` holds a locale KEY; whoever sets it also clears it (~3s).
 * Non-interactive on purpose — no data-input-region, clicks fall through.
 */
export function Toast() {
  const notice = useSmabar((s) => s.notice);
  if (notice === null) return null;

  return (
    <div
      className="surface-flyout max-w-[calc(var(--sb-work-area-width,100vw)-2*var(--sb-space-l))] rounded-[var(--sb-radius-m)] px-4 py-2 text-xs text-[color:var(--sb-text)]"
      data-shell-toast
      role="status"
    >
      {t(notice)}
    </div>
  );
}
