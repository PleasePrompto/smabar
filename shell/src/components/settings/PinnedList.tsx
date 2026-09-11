import { Check, Minus, Pencil, X } from "lucide-react";
import { useState } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import { shortcutDisplayLabel, useSmabar } from "../../store/bar";
import { IconOrInitial, MoveButtons } from "./controls";
import { moveItem } from "./model";
import { setConfig } from "./persist";

/**
 * The pinned shortcuts: reorder, rename, remove.
 *
 * `entries` (raw config) is index-aligned with `pinned` (resolved for
 * display), so both a move and a rename edit the raw array at the same index
 * and write it back whole — the same path the drag reorder already uses.
 */
export function PinnedList() {
  const shortcuts = useSmabar((s) => s.shortcuts);
  const [editing, setEditing] = useState<string | null>(null);

  const write = (entries: readonly unknown[]) => {
    setConfig("shortcuts.pinned", entries);
  };
  const move = (from: number, to: number) => {
    write(moveItem(shortcuts.entries, from, to));
  };
  const rename = (index: number, label: string) => {
    const trimmed = label.trim();
    write(
      shortcuts.entries.map((entry, at) => {
        if (at !== index) return entry;
        const next = { ...entry };
        // An emptied field means "no name of my own": drop the override so
        // the core resolves the desktop entry's name or the URL host again.
        if (trimmed === "") delete next.label;
        else next.label = trimmed;
        return next;
      }),
    );
    setEditing(null);
  };
  const unpin = (id: string) => {
    void call("unpin_shortcut", { id }).catch(reportError);
  };

  return (
    <div className="sb-list">
      {shortcuts.pinned.map((pinned, index) => {
        const label = shortcutDisplayLabel(pinned, shortcuts.entries[index], t);
        return (
          <div key={pinned.id} className="sb-row">
            {pinned.separator ? (
              <Minus size="1em" aria-hidden="true" />
            ) : (
              <IconOrInitial icon={pinned.icons[0]} label={label} />
            )}
            {editing === pinned.id ? (
              <RenameField
                value={label}
                onCommit={(label) => {
                  rename(index, label);
                }}
                onCancel={() => {
                  setEditing(null);
                }}
              />
            ) : (
              <>
                <span
                  className="settings-ellipsis"
                  style={{ flex: 1, minWidth: 0 }}
                >
                  {pinned.separator ? t("settings.shortcuts.separator") : label}
                </span>
                {!pinned.separator && (
                  <button
                    className="sb-btn sb-btn-ghost sb-btn-icon"
                    aria-label={t("settings.shortcuts.rename")}
                    title={t("settings.shortcuts.rename")}
                    onClick={() => {
                      setEditing(pinned.id);
                    }}
                  >
                    <Pencil size="1em" />
                  </button>
                )}
                <MoveButtons
                  index={index}
                  count={shortcuts.pinned.length}
                  onMove={move}
                />
                <button
                  className="sb-btn sb-btn-ghost sb-btn-icon"
                  aria-label={t("settings.shortcuts.unpin")}
                  title={t("settings.shortcuts.unpin")}
                  onClick={() => {
                    unpin(pinned.id);
                  }}
                >
                  <X size="1em" />
                </button>
              </>
            )}
          </div>
        );
      })}
      {shortcuts.pinned.length === 0 && (
        <div className="sb-faint">{t("shortcuts.empty")}</div>
      )}
    </div>
  );
}

function RenameField({
  value,
  onCommit,
  onCancel,
}: {
  value: string;
  onCommit: (label: string) => void;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState(value);
  return (
    <>
      <input
        className="sb-input"
        style={{ flex: 1, minWidth: 0 }}
        // The field only exists because the rename button was just pressed;
        // without focus the next keystroke would go nowhere.
        autoFocus
        value={draft}
        aria-label={t("settings.shortcuts.rename")}
        placeholder={value}
        onChange={(e) => {
          setDraft(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") onCommit(draft);
          if (e.key === "Escape") {
            // The panel closes on Escape from a window listener. Without
            // stopping here, aborting a rename threw the whole panel away.
            e.stopPropagation();
            onCancel();
          }
        }}
      />
      <button
        className="sb-btn sb-btn-ghost sb-btn-icon"
        aria-label={t("settings.shortcuts.renameSave")}
        title={t("settings.shortcuts.renameSave")}
        onClick={() => {
          onCommit(draft);
        }}
      >
        <Check size="1em" />
      </button>
      <button
        className="sb-btn sb-btn-ghost sb-btn-icon"
        aria-label={t("settings.cancel")}
        title={t("settings.cancel")}
        onClick={onCancel}
      >
        <X size="1em" />
      </button>
    </>
  );
}
