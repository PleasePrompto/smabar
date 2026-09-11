import { FileDown, Save, Trash2 } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError, visibleError } from "../../ipc/log";
import { showNotice } from "../../ipc/surface";
import { useSmabar, type ThemeSummary } from "../../store/bar";
import { ConfirmRow, SettingGroup, SettingRow } from "./controls";
import { slugifyThemeName, themeDisplayName } from "./model";
import { setConfigDebounced } from "./persist";
import { ThemeImportSettings } from "./ThemeImportSettings";

interface ExportDirInfo {
  configured: string;
  effective: string | null;
  error: string | null;
}

type ThemeAction = "save" | "delete" | "export";

export function ThemeManager({
  themes,
  onThemes,
}: {
  themes: ThemeSummary[];
  onThemes: (themes: ThemeSummary[]) => void;
}) {
  const active = useSmabar((state) => state.theme);
  const [nameInput, setNameInput] = useState("");
  const [saveConfirm, setSaveConfirm] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const [lastExport, setLastExport] = useState<string | null>(null);
  const [exportDir, setExportDir] = useState<ExportDirInfo | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingActions, setPendingActions] = useState<
    ReadonlySet<ThemeAction>
  >(new Set());
  const pendingActionsRef = useRef(new Set<ThemeAction>());
  const saveButton = useRef<HTMLButtonElement>(null);
  const nameField = useRef<HTMLInputElement>(null);
  const customThemes = useRef<HTMLDivElement>(null);
  const nameHintId = useId();

  const beginAction = useCallback((action: ThemeAction) => {
    if (pendingActionsRef.current.has(action)) return false;
    const next = new Set(pendingActionsRef.current).add(action);
    pendingActionsRef.current = next;
    setPendingActions(next);
    return true;
  }, []);
  const endAction = useCallback((action: ThemeAction) => {
    const next = new Set(pendingActionsRef.current);
    next.delete(action);
    pendingActionsRef.current = next;
    setPendingActions(next);
  }, []);

  useEffect(() => {
    call<ExportDirInfo>("get_theme_export_dir")
      .then(setExportDir)
      .catch((error: unknown) => {
        setExportDir({
          configured: "",
          effective: null,
          error: visibleError(error),
        });
        reportError(error);
      });
  }, []);

  const slug = slugifyThemeName(nameInput);
  const collision =
    slug === null ? undefined : themes.find((theme) => theme.name === slug);
  const nameIsBundled = collision?.source === "bundled";
  const nameError =
    nameInput.trim() === ""
      ? null
      : slug === null
        ? t("settings.themes.nameInvalid")
        : nameIsBundled
          ? t("settings.themes.nameBundled")
          : null;

  const runSave = (overwrite: boolean) => {
    if (slug === null || !beginAction("save")) return;
    let failed = false;
    setSaveConfirm(false);
    setActionError(null);
    call<ThemeSummary[]>("save_custom_theme", { name: slug, overwrite })
      .then((list) => {
        onThemes(list);
        setNameInput("");
        requestAnimationFrame(() => nameField.current?.focus());
        void showNotice("settings.themes.saved").catch(reportError);
      })
      .catch((error: unknown) => {
        failed = true;
        setActionError(visibleError(error));
        reportError(error);
      })
      .finally(() => {
        endAction("save");
        if (failed) requestAnimationFrame(() => saveButton.current?.focus());
      });
  };
  const onSaveClick = () => {
    if (slug === null || nameIsBundled) return;
    if (collision !== undefined) {
      setSaveConfirm(true);
      return;
    }
    runSave(false);
  };

  const runDelete = (name: string) => {
    if (!beginAction("delete")) return;
    let failed = false;
    setDeleteConfirm(null);
    setActionError(null);
    call<ThemeSummary[]>("delete_theme", { name })
      .then((list) => {
        onThemes(list);
        requestAnimationFrame(() => customThemes.current?.focus());
        void showNotice("settings.themes.deleted").catch(reportError);
      })
      .catch((error: unknown) => {
        failed = true;
        setActionError(visibleError(error));
        reportError(error);
      })
      .finally(() => {
        endAction("delete");
        if (failed) {
          requestAnimationFrame(() =>
            customThemes.current
              ?.querySelector<HTMLButtonElement>(
                `[data-theme-delete="${name}"]`,
              )
              ?.focus(),
          );
        }
      });
  };

  const runExport = (name: string, returnFocus: HTMLButtonElement) => {
    if (!beginAction("export")) return;
    setActionError(null);
    call<string>("export_theme", {
      name,
      directory: exportDir?.configured,
    })
      .then((path) => {
        setLastExport(path);
        void showNotice("settings.themes.exported").catch(reportError);
      })
      .catch((error: unknown) => {
        setActionError(visibleError(error));
        void showNotice("settings.themes.exportFailed").catch(reportError);
        reportError(error);
      })
      .finally(() => {
        endAction("export");
        requestAnimationFrame(() => {
          if (returnFocus.isConnected) returnFocus.focus();
        });
      });
  };

  const dropins = themes.filter((theme) => theme.source === "dropin");

  return (
    // Its own settings block (and thus its own sub-navigation entry): one
    // single-purpose row each for save, saved list, export, and import.
    <SettingGroup title={t("settings.themes.group")}>
      {actionError !== null && (
        <div className="sb-crit" role="alert">
          {actionError}
        </div>
      )}
      <SettingRow
        label={t("settings.themes.saveTitle")}
        description={t("settings.themes.saveDescription")}
        wide
      >
        <div className="sb-inline">
          <input
            ref={nameField}
            className="sb-input"
            style={{ flex: 1, minWidth: 0 }}
            value={nameInput}
            placeholder={t("settings.themes.savePlaceholder")}
            aria-label={t("settings.themes.saveTitle")}
            aria-invalid={nameError === null ? undefined : true}
            aria-describedby={nameError === null ? undefined : nameHintId}
            onChange={(e) => {
              setNameInput(e.target.value);
              setSaveConfirm(false);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") onSaveClick();
            }}
          />
          <button
            ref={saveButton}
            className="sb-btn sb-btn-ghost"
            disabled={
              slug === null || nameIsBundled || pendingActions.has("save")
            }
            onClick={onSaveClick}
          >
            <Save size="1em" aria-hidden="true" />
            {t("settings.themes.save")}
          </button>
        </div>
        {nameError !== null && (
          <div id={nameHintId} className="sb-crit" role="alert">
            {nameError}
          </div>
        )}
        {saveConfirm && (
          <ConfirmRow
            label={t("settings.themes.saveTitle")}
            question={t("settings.themes.saveOverwrite")}
            action={t("settings.themes.overwrite")}
            onCancel={() => {
              setSaveConfirm(false);
            }}
            onConfirm={() => {
              runSave(true);
            }}
            returnFocus={() => {
              saveButton.current?.focus();
            }}
          />
        )}
      </SettingRow>

      <SettingRow label={t("settings.themes.customTitle")} wide>
        {dropins.length === 0 ? (
          <div ref={customThemes} className="sb-faint" tabIndex={-1}>
            {t("settings.themes.customEmpty")}
          </div>
        ) : (
          <div ref={customThemes} className="sb-list" tabIndex={-1}>
            {dropins.map((theme) =>
              deleteConfirm === theme.name ? (
                <ConfirmRow
                  key={theme.name}
                  label={themeDisplayName(theme)}
                  question={t("settings.themes.deleteConfirm")}
                  action={t("settings.themes.delete")}
                  onCancel={() => {
                    setDeleteConfirm(null);
                  }}
                  onConfirm={() => {
                    runDelete(theme.name);
                  }}
                  returnFocus={() => {
                    customThemes.current
                      ?.querySelector<HTMLButtonElement>(
                        `[data-theme-delete="${theme.name}"]`,
                      )
                      ?.focus();
                  }}
                />
              ) : (
                <div className="sb-row" key={theme.name}>
                  <span
                    className="settings-ellipsis"
                    style={{ flex: 1, minWidth: 0 }}
                  >
                    {themeDisplayName(theme)}
                  </span>
                  <span className="sb-badge">
                    {t("settings.themes.custom")}
                  </span>
                  <button
                    className="sb-btn sb-btn-ghost sb-btn-icon"
                    aria-label={t("settings.themes.export")}
                    title={t("settings.themes.export")}
                    disabled={pendingActions.has("export")}
                    onClick={(event) => {
                      runExport(theme.name, event.currentTarget);
                    }}
                  >
                    <FileDown size="1em" />
                  </button>
                  <button
                    data-theme-delete={theme.name}
                    className="sb-btn sb-btn-ghost sb-btn-icon"
                    aria-label={t("settings.themes.delete")}
                    title={t("settings.themes.delete")}
                    disabled={pendingActions.has("delete")}
                    onClick={() => {
                      setDeleteConfirm(theme.name);
                    }}
                  >
                    <Trash2 size="1em" />
                  </button>
                </div>
              ),
            )}
          </div>
        )}
      </SettingRow>

      <SettingRow
        label={t("settings.themes.export")}
        description={t("settings.themes.exportDescription")}
        wide
      >
        <div className="sb-inline">
          <button
            className="sb-btn sb-btn-ghost"
            disabled={pendingActions.has("export")}
            onClick={(event) => {
              runExport(active, event.currentTarget);
            }}
          >
            <FileDown size="1em" aria-hidden="true" />
            {t("settings.themes.activeExport")}
          </button>
        </div>
        {lastExport !== null && (
          <div className="sb-faint settings-ellipsis" title={lastExport}>
            {lastExport}
          </div>
        )}
        <div className="sb-inline">
          <input
            className="sb-input"
            style={{ flex: 1, minWidth: 0 }}
            value={exportDir?.configured ?? ""}
            placeholder={exportDir?.effective ?? ""}
            aria-label={t("settings.themes.exportDir")}
            disabled={exportDir === null}
            onChange={(e) => {
              const value = e.target.value;
              setExportDir((dir) =>
                dir === null ? dir : { ...dir, configured: value, error: null },
              );
              setConfigDebounced("themeExportDir", value);
            }}
          />
        </div>
        <div className="sb-faint">
          {t("settings.themes.exportDirDescription")}
        </div>
        {exportDir?.error !== null && exportDir?.error !== undefined && (
          <div className="sb-crit" role="alert">
            {exportDir.error}
          </div>
        )}
      </SettingRow>

      <ThemeImportSettings themes={themes} onThemes={onThemes} />
    </SettingGroup>
  );
}
