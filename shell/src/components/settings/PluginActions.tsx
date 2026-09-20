import { Eye, EyeOff, Power, Trash2 } from "lucide-react";
import { useRef, useState } from "react";

import { t } from "../../i18n/t";
import { useSmabar } from "../../store/bar";
import { ConfirmRow } from "./controls";
import type { PluginManagement } from "./usePluginManagement";

/** The same actions and confirmation in the compact list and the cards. */
export function PluginActions({
  pluginId,
  name,
  tiles,
  tileCount,
  management,
  compact = false,
}: {
  pluginId: string;
  name: string;
  tiles: { id: string; name: string }[];
  tileCount: number;
  management: PluginManagement;
  compact?: boolean;
}) {
  const hidden = useSmabar((state) => state.pluginsHidden);
  const off = useSmabar((state) => state.pluginsDeactivated.includes(pluginId));
  const [confirming, setConfirming] = useState(false);
  const deleteButton = useRef<HTMLButtonElement>(null);
  const activeLabel = t(
    off ? "settings.plugins.switchOn" : "settings.plugins.switchOff",
  );
  const scope = (label: string) =>
    tileCount > 1
      ? `${label} — ${t("settings.plugins.affectsAll").replace("{count}", String(tileCount))}`
      : label;
  const buttonClass = `sb-btn sb-btn-ghost${compact ? " sb-btn-icon" : ""}`;

  return (
    <fieldset
      className={`settings-plugin-actions${compact ? " settings-plugin-actions-compact" : ""}`}
      disabled={management.busy}
      aria-label={t("settings.plugins.actions").replace("{name}", name)}
      aria-busy={management.busy}
      onKeyDown={(event) => {
        if (event.key !== "Escape" || !confirming) return;
        event.stopPropagation();
        setConfirming(false);
        queueMicrotask(() => deleteButton.current?.focus());
      }}
    >
      {!confirming && (
        <>
          {tiles.map((tile) => {
            const isHidden = hidden.includes(tile.id);
            const label = t(
              isHidden ? "settings.plugins.show" : "settings.plugins.hide",
            );
            const accessibleLabel =
              tileCount > 1 ? `${label}: ${t(tile.name)}` : label;
            return (
              <button
                key={tile.id}
                type="button"
                className={buttonClass}
                aria-label={accessibleLabel}
                title={accessibleLabel}
                aria-pressed={!isHidden}
                onClick={() => {
                  void management.toggleHidden(pluginId, tile.id);
                }}
              >
                {isHidden ? (
                  <EyeOff size="1em" aria-hidden="true" />
                ) : (
                  <Eye size="1em" aria-hidden="true" />
                )}
                {!compact && accessibleLabel}
              </button>
            );
          })}
          <button
            type="button"
            className={buttonClass}
            aria-label={scope(activeLabel)}
            title={scope(activeLabel)}
            aria-pressed={!off}
            onClick={() => {
              void management.toggleActive(pluginId);
            }}
          >
            <Power size="1em" aria-hidden="true" />
            {!compact && activeLabel}
          </button>
        </>
      )}
      <button
        ref={deleteButton}
        type="button"
        className={`${buttonClass} settings-plugin-delete`}
        aria-label={scope(t("settings.plugins.delete"))}
        title={scope(t("settings.plugins.delete"))}
        hidden={confirming}
        onClick={() => {
          setConfirming(true);
        }}
      >
        <Trash2 size="1em" aria-hidden="true" />
        {!compact && t("settings.plugins.deleteConfirmAction")}
      </button>
      {confirming && (
        <ConfirmRow
          label={name}
          question={t("settings.plugins.deleteConfirm").replace("{name}", name)}
          action={t("settings.plugins.deleteConfirmAction")}
          onCancel={() => {
            setConfirming(false);
          }}
          returnFocus={() => {
            deleteButton.current?.focus();
          }}
          onConfirm={() => {
            void management.remove(pluginId).then((removed) => {
              if (!removed) return;
              useSmabar.getState().setSettingsGroup("plugins");
              requestAnimationFrame(() => {
                document.getElementById("settings-content")?.focus();
              });
            });
          }}
        />
      )}
      {management.failedPlugin === pluginId && (
        <p className="sb-error" role="alert">
          {t("settings.plugins.actionFailed")}
        </p>
      )}
    </fieldset>
  );
}
