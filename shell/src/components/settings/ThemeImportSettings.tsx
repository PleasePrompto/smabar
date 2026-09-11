import { FileUp } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError, visibleError } from "../../ipc/log";
import { showNotice } from "../../ipc/surface";
import { useSmabar, type ThemeSummary } from "../../store/bar";
import { ConfirmRow, SettingRow } from "./controls";
import { slugifyThemePath } from "./model";

interface ThemeImportSettingsProps {
  themes: ThemeSummary[];
  onThemes: (themes: ThemeSummary[]) => void;
}

export function ThemeImportSettings({
  themes,
  onThemes,
}: ThemeImportSettingsProps) {
  const [importPath, setImportPath] = useState("");
  const [importConfirm, setImportConfirm] = useState<string | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [importPending, setImportPending] = useState(false);
  const importPendingRef = useRef(false);
  const importButton = useRef<HTMLButtonElement>(null);
  const importField = useRef<HTMLInputElement>(null);

  const runImport = useCallback(
    (path: string, overwrite: boolean) => {
      if (importPendingRef.current) return;
      let failed = false;
      importPendingRef.current = true;
      setImportPending(true);
      setImportError(null);
      setImportConfirm(null);
      call<ThemeSummary[]>("import_theme", { path, overwrite })
        .then((list) => {
          onThemes(list);
          setImportPath("");
          requestAnimationFrame(() => importField.current?.focus());
          void showNotice("settings.themes.imported").catch(reportError);
        })
        .catch((error: unknown) => {
          failed = true;
          setImportError(visibleError(error));
          reportError(error);
        })
        .finally(() => {
          importPendingRef.current = false;
          setImportPending(false);
          if (failed) {
            requestAnimationFrame(() => importButton.current?.focus());
          }
        });
    },
    [onThemes],
  );

  const startImport = useCallback(
    (path: string) => {
      const trimmed = path.trim();
      if (trimmed === "") return;
      setImportError(null);
      const name = slugifyThemePath(trimmed);
      const existing =
        name === null ? undefined : themes.find((theme) => theme.name === name);
      if (existing?.source === "bundled") {
        setImportError(t("settings.themes.nameBundled"));
        return;
      }
      if (existing !== undefined) {
        setImportConfirm(trimmed);
        return;
      }
      runImport(trimmed, false);
    },
    [themes, runImport],
  );

  useEffect(() => {
    const consume = (path: string | null) => {
      if (path === null) return;
      useSmabar.getState().setThemeImportPath(null);
      startImport(path);
    };
    const pending = useSmabar.getState().themeImportPath;
    if (pending !== null) {
      queueMicrotask(() => {
        consume(pending);
      });
    }
    return useSmabar.subscribe((state, previous) => {
      if (state.themeImportPath !== previous.themeImportPath) {
        consume(state.themeImportPath);
      }
    });
  }, [startImport]);

  return (
    <SettingRow
      label={t("settings.themes.import")}
      description={t("settings.themes.importPathDescription")}
      wide
    >
      <div className="sb-inline">
        <input
          ref={importField}
          className="sb-input"
          style={{ flex: 1, minWidth: 0 }}
          value={importPath}
          placeholder={t("settings.themes.importPlaceholder")}
          aria-label={t("settings.themes.importPath")}
          onChange={(event) => {
            setImportPath(event.target.value);
            setImportError(null);
            setImportConfirm(null);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") startImport(importPath);
          }}
        />
        <button
          ref={importButton}
          className="sb-btn sb-btn-ghost"
          disabled={importPath.trim() === "" || importPending}
          onClick={() => {
            startImport(importPath);
          }}
        >
          <FileUp size="1em" aria-hidden="true" />
          {t("settings.themes.import")}
        </button>
      </div>
      {importError !== null && (
        <div className="sb-crit" role="alert">
          {importError}
        </div>
      )}
      {importConfirm !== null && (
        <ConfirmRow
          label={t("settings.themes.import")}
          question={t("settings.themes.importOverwrite")}
          action={t("settings.themes.overwrite")}
          onCancel={() => {
            setImportConfirm(null);
          }}
          onConfirm={() => {
            runImport(importConfirm, true);
          }}
          returnFocus={() => {
            importButton.current?.focus();
          }}
        />
      )}
    </SettingRow>
  );
}
