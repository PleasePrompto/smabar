import { Check, FileDown, RotateCcw, Save, Trash2 } from "lucide-react";
import { useId, useRef, useState } from "react";
import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError, visibleError } from "../../ipc/log";
import { useSmabar, type ThemeSummary } from "../../store/bar";
import { ConfirmRow, SettingGroup } from "./controls";
import { slugifyThemeName, themeDisplayName } from "./model";
import { COLOR_TOKENS } from "./model";
import { FONT_TOKENS } from "../../theme/fonts";
import { dropTokens } from "./tokens";
import { flushConfig, setConfig } from "./persist";
import { ThemeImportSettings } from "./ThemeImportSettings";
import { StoreUpdateLink } from "./UpdateBadge";
import { ThemePreview, ThemeSwatches } from "./ThemePreview";

interface SavedCopy {
  themes: ThemeSummary[];
  document: string;
  path: string;
  fileError: string | null;
}
export function ThemeManager({
  themes,
  onThemes,
}: {
  themes: ThemeSummary[];
  onThemes: (themes: ThemeSummary[]) => void;
}) {
  const active = useSmabar((state) => state.theme);
  const previewId = useId();
  const customized = useSmabar((state) =>
    [...COLOR_TOKENS, ...FONT_TOKENS].some(
      (key) => key in state.appearance.tokens,
    ),
  );
  const [name, setName] = useState("");
  const [confirm, setConfirm] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<SavedCopy | null>(null);
  const saveButton = useRef<HTMLButtonElement>(null);
  const slug = slugifyThemeName(name);
  const collision = themes.find((theme) => theme.name === slug);
  async function perform(operation: () => Promise<void>) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    setConfirm(null);
    try {
      await operation();
    } catch (cause: unknown) {
      setError(visibleError(cause));
      reportError(cause);
    } finally {
      pending.current = false;
      setBusy(false);
      requestAnimationFrame(() => saveButton.current?.focus());
    }
  }
  const chooseTarget = (filename: string) =>
    call<string | null>("choose_settings_file", {
      purpose: "themeSave",
      suggested: `${filename}.json`,
    });
  const save = (overwrite: boolean) =>
    perform(async () => {
      if (slug === null) return;
      const path = await chooseTarget(slug);
      if (path === null) return;
      await flushConfig();
      const result = await call<SavedCopy>("save_theme_copy", {
        name: slug,
        overwrite,
        path,
      });
      onThemes(result.themes);
      setSaved(result);
      setName("");
    });
  const copy = (theme: string) =>
    perform(async () => {
      const path = await chooseTarget(theme);
      if (path === null) return;
      const document = await call<string>("theme_file_document", {
        name: theme,
      });
      await call("write_theme_copy", { path, document });
      setSaved({ themes, document, path, fileError: null });
    });
  return (
    <SettingGroup title={t("settings.design.theme")} updateKey="theme">
      <div className="settings-theme-toolbar">
        <label className="settings-theme-name">
          <span>{t("settings.themes.saveTitle")}</span>
          <input
            className="sb-input"
            aria-label={t("settings.themes.saveTitle")}
            value={name}
            placeholder={t("settings.themes.savePlaceholder")}
            onChange={(event) => {
              setName(event.target.value);
              setConfirm(null);
            }}
          />
        </label>
        <button
          ref={saveButton}
          type="button"
          className="sb-btn sb-btn-primary"
          disabled={busy || slug === null || collision?.source === "bundled"}
          onClick={() => {
            if (collision !== undefined) setConfirm("@save");
            else void save(false);
          }}
        >
          <Save size="1em" />
          {t("settings.themes.save")}
        </button>
        <ThemeImportSettings themes={themes} onThemes={onThemes} />
      </div>
      <p className="settings-help">{t("settings.themes.saveDescription")}</p>
      {name.trim() !== "" &&
        (slug === null || collision?.source === "bundled") && (
          <p role="alert" className="sb-crit">
            {t(
              slug === null
                ? "settings.themes.nameInvalid"
                : "settings.themes.nameBundled",
            )}
          </p>
        )}
      {confirm === "@save" && (
        <ConfirmRow
          label={t("settings.themes.save")}
          question={t("settings.themes.saveOverwrite")}
          action={t("settings.themes.overwrite")}
          onCancel={() => {
            setConfirm(null);
          }}
          onConfirm={() => {
            void save(true);
          }}
        />
      )}
      {error !== null && (
        <p role="alert" className="sb-crit">
          {error}
        </p>
      )}
      {saved !== null && (
        <div
          role="status"
          className={saved.fileError === null ? "settings-help" : "sb-crit"}
        >
          {t(
            saved.fileError === null
              ? "settings.themes.savedCopy"
              : "settings.themes.copyFailed",
          )}{" "}
          <span className="settings-file-path">{saved.path}</span>
          {saved.fileError !== null && (
            <>
              <p>{saved.fileError}</p>
              <button
                type="button"
                className="sb-btn"
                disabled={busy}
                onClick={() => {
                  void perform(async () => {
                    const path = await chooseTarget(
                      slugifyThemeName(
                        saved.path
                          .split(/[\\/]/)
                          .pop()
                          ?.replace(/\.json$/i, "") ?? "theme",
                      ) ?? "theme",
                    );
                    if (path === null) return;
                    await call("write_theme_copy", {
                      path,
                      document: saved.document,
                    });
                    setSaved({ ...saved, path, fileError: null });
                  });
                }}
              >
                {t("settings.themes.retryCopy")}
              </button>
            </>
          )}
        </div>
      )}
      {customized && (
        <div className="sb-inline">
          <span className="sb-badge sb-badge-warn">
            {t("settings.design.customized")}
          </span>
          <button
            type="button"
            className="sb-btn sb-btn-ghost"
            onClick={() => {
              dropTokens([...COLOR_TOKENS, ...FONT_TOKENS]);
            }}
          >
            <RotateCcw size="1em" />
            {t("settings.design.resetOverrides")}
          </button>
        </div>
      )}
      <div className="settings-theme-library">
        {themes.map((theme) => (
          <article
            key={theme.name}
            className="settings-theme-card"
            data-active={theme.name === active || undefined}
          >
            <button
              type="button"
              className="settings-theme-select"
              aria-pressed={theme.name === active}
              aria-label={themeDisplayName(theme)}
              aria-describedby={`${previewId}-${theme.name}`}
              onClick={() => {
                setConfig("theme", theme.name);
              }}
            >
              <span className="settings-theme-heading">
                <span>{themeDisplayName(theme)}</span>
                <small>
                  {theme.name === active && (
                    <Check size="1em" aria-hidden="true" />
                  )}
                  {t(
                    theme.name === active
                      ? "settings.themes.active"
                      : theme.source === "bundled"
                        ? "settings.themes.bundled"
                        : "settings.themes.custom",
                  )}
                </small>
              </span>
              <ThemePreview
                preview={theme.preview}
                descriptionId={`${previewId}-${theme.name}`}
              />
            </button>
            <div className="settings-theme-actions">
              <ThemeSwatches preview={theme.preview} />
              <StoreUpdateLink kind="theme" id={theme.name} />
              <button
                type="button"
                className="sb-btn sb-btn-ghost sb-btn-icon"
                disabled={busy}
                aria-label={`${t("settings.themes.export")}: ${themeDisplayName(theme)}`}
                onClick={() => {
                  void copy(theme.name);
                }}
              >
                <FileDown size="1em" />
              </button>
              {theme.source === "dropin" && (
                <button
                  type="button"
                  className="sb-btn sb-btn-ghost sb-btn-icon"
                  disabled={busy}
                  aria-label={`${t("settings.themes.delete")}: ${themeDisplayName(theme)}`}
                  onClick={() => {
                    setConfirm(theme.name);
                  }}
                >
                  <Trash2 size="1em" />
                </button>
              )}
            </div>
            {confirm === theme.name && (
              <ConfirmRow
                label={themeDisplayName(theme)}
                question={t("settings.themes.deleteConfirm")}
                action={t("settings.themes.delete")}
                onCancel={() => {
                  setConfirm(null);
                }}
                onConfirm={() => {
                  void perform(async () => {
                    onThemes(
                      await call<ThemeSummary[]>("delete_theme", {
                        name: theme.name,
                      }),
                    );
                  });
                }}
              />
            )}
          </article>
        ))}
      </div>
    </SettingGroup>
  );
}
