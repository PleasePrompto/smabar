import { t } from "../../i18n/t";
import type { StoreKind } from "../../ipc/store";
import { useSmabar } from "../../store/bar";
import { updateLabel } from "./storeModel";

export function UpdateDot({
  label = t("settings.update.badge"),
}: {
  label?: string;
}) {
  return (
    <span
      className="settings-update-dot"
      role="img"
      aria-label={label}
      title={label}
    />
  );
}

/** Always opens the existing detail/confirmation flow, including blocked updates. */
export function StoreUpdateLink({
  kind,
  id,
  compact = false,
}: {
  kind: StoreKind;
  id: string;
  compact?: boolean;
}) {
  const entry = useSmabar((state) =>
    state.communityUpdates.find(
      (entry) => entry.kind === kind && entry.id === id,
    ),
  );
  const open = useSmabar((state) => state.openStoreEntry);
  if (entry === undefined) return null;
  const label = `${entry.name}: ${updateLabel(entry)}`;
  return (
    <button
      type="button"
      className="sb-btn sb-btn-ghost settings-update-link"
      aria-label={label}
      title={label}
      onClick={() => {
        open(kind, id);
      }}
    >
      <UpdateDot label={label} />
      {!compact && updateLabel(entry)}
    </button>
  );
}
