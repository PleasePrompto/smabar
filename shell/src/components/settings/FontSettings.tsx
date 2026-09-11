import { Check, Download, LoaderCircle, RotateCcw, Search } from "lucide-react";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import { useSmabar, type ThemeSummary } from "../../store/bar";
import {
  ensureManagedFont,
  fontFamilyStack,
  fontSourceToken,
  fontToken,
  getFontInstallState,
  googleFontId,
  isGenericFontFamily,
  subscribeFontInstall,
  type FontOption,
  type FontSlot,
  type FontSource,
} from "../../theme/fonts";
import { SettingRow } from "./controls";
import { dropTokens, writeTokens } from "./tokens";

export function FontSettings({ themes }: { themes: ThemeSummary[] }) {
  return (
    <>
      <FontRow slot="sans" themes={themes} />
      <FontRow slot="mono" themes={themes} />
    </>
  );
}

function FontRow({ slot, themes }: { slot: FontSlot; themes: ThemeSummary[] }) {
  const activeTheme = useSmabar((state) => state.theme);
  const familyOverride = useSmabar(
    (state) => state.appearance.tokens[fontToken(slot)],
  );
  const sourceOverride = useSmabar(
    (state) => state.appearance.tokens[fontSourceToken(slot)],
  );
  const [open, setOpen] = useState(false);
  const [sourceFilter, setSourceFilter] = useState<FontSource>("system");
  const [query, setQuery] = useState("");
  const [listResult, setListResult] = useState<{
    key: string;
    options: FontOption[];
    error: boolean;
  } | null>(null);
  const [failedOption, setFailedOption] = useState<FontOption | null>(null);
  const [pendingSelection, setPendingSelection] = useState<{
    id: string;
    request: number;
    context: string;
  } | null>(null);
  const [systemCheck, setSystemCheck] = useState<{
    family: string;
    available: boolean;
  } | null>(null);
  const selection = useRef(0);

  const themeFont = themes.find((theme) => theme.name === activeTheme)?.fonts[
    slot
  ];
  const family =
    familyOverride ??
    themeFont?.family ??
    getComputedStyle(document.documentElement)
      .getPropertyValue(fontToken(slot))
      .trim();
  const source =
    (sourceOverride ??
      themeFont?.source ??
      document.documentElement.style
        .getPropertyValue(fontSourceToken(slot))
        .trim()) ||
    "system";
  const googleId = googleFontId(source);
  const installState = useSyncExternalStore(
    subscribeFontInstall,
    () => getFontInstallState(googleId),
    () => getFontInstallState(googleId),
  );
  const customized =
    familyOverride !== undefined || sourceOverride !== undefined;
  const currentName = firstFamily(family) || t("settings.fonts.system");
  const label = t(`settings.fonts.${slot}`);
  const selectionContext = `${activeTheme}\u0000${familyOverride ?? ""}\u0000${sourceOverride ?? ""}`;
  const pendingOptionId =
    pendingSelection?.context === selectionContext ? pendingSelection.id : null;
  const systemAvailable =
    source !== "system"
      ? null
      : isGenericFontFamily(currentName)
        ? true
        : systemCheck?.family === currentName
          ? systemCheck.available
          : null;
  const listKey = `${sourceFilter}\u0000${slot}\u0000${query.trim()}`;
  const loading = open && listResult?.key !== listKey;
  const listError = listResult?.key === listKey && listResult.error;
  const options = listResult?.key === listKey ? listResult.options : [];

  useEffect(() => {
    selection.current += 1;
  }, [selectionContext]);

  useEffect(() => {
    if (source !== "system" || isGenericFontFamily(currentName)) return;
    let current = true;
    call<FontOption[]>("font_list", {
      query: currentName,
      source: "system",
      monospaced: slot === "mono" ? true : undefined,
      limit: 100,
    })
      .then((fonts) => {
        if (!current) return;
        setSystemCheck({
          family: currentName,
          available: fonts.some(
            (font) =>
              font.family.localeCompare(currentName, undefined, {
                sensitivity: "accent",
              }) === 0,
          ),
        });
      })
      .catch((error: unknown) => {
        reportError(error);
      });
    return () => {
      current = false;
    };
  }, [currentName, slot, source]);

  useEffect(() => {
    if (!open) return;
    let current = true;
    call<FontOption[]>("font_list", {
      query: query.trim() || undefined,
      source: sourceFilter,
      monospaced: slot === "mono" ? true : undefined,
      limit: query.trim() === "" && sourceFilter === "google" ? 24 : 100,
    })
      .then((fonts) => {
        if (current)
          setListResult({ key: listKey, options: fonts, error: false });
      })
      .catch((error: unknown) => {
        if (!current) return;
        setListResult({ key: listKey, options: [], error: true });
        reportError(error);
      });
    return () => {
      current = false;
    };
  }, [listKey, open, query, slot, sourceFilter]);

  const choose = (option: FontOption) => {
    setFailedOption(null);
    const request = ++selection.current;
    const themeBefore = activeTheme;
    const familyBefore = familyOverride;
    const sourceBefore = sourceOverride;
    const contextStillActive = () => {
      const current = useSmabar.getState();
      return (
        current.theme === themeBefore &&
        current.appearance.tokens[fontToken(slot)] === familyBefore &&
        current.appearance.tokens[fontSourceToken(slot)] === sourceBefore
      );
    };
    if (option.source === "system") {
      setPendingSelection(null);
      writeTokens({
        [fontToken(slot)]: fontFamilyStack(
          option.family,
          slot,
          option.category,
        ),
        [fontSourceToken(slot)]: "system",
      });
      setOpen(false);
      return;
    }
    setPendingSelection({ id: option.id, request, context: selectionContext });
    void ensureManagedFont(option.id)
      .then((font) => {
        if (request !== selection.current || !contextStillActive()) {
          setPendingSelection((current) =>
            current?.request === request ? null : current,
          );
          return;
        }
        writeTokens({
          [fontToken(slot)]: fontFamilyStack(
            font.family,
            slot,
            option.category,
          ),
          [fontSourceToken(slot)]: `google:${font.id}`,
        });
        setPendingSelection(null);
        setOpen(false);
      })
      .catch((error: unknown) => {
        if (request !== selection.current || !contextStillActive()) {
          setPendingSelection((current) =>
            current?.request === request ? null : current,
          );
          return;
        }
        setPendingSelection(null);
        setFailedOption(option);
        reportError(error);
      });
  };

  return (
    <SettingRow
      label={label}
      description={t(`settings.fonts.${slot}Description`)}
      wide
    >
      <div className="settings-font-control">
        <button
          type="button"
          className="settings-font-current"
          aria-expanded={open}
          onClick={() => {
            setOpen((value) => !value);
          }}
        >
          <span
            className="settings-font-sample"
            style={{ fontFamily: family || undefined }}
            aria-hidden="true"
          >
            {slot === "mono" ? "01" : "Aa"}
          </span>
          <span className="settings-font-current-copy">
            <strong>{currentName}</strong>
            <small>
              {sourceLabel(source, installState.status, systemAvailable)}
            </small>
          </span>
          <span className="sb-badge settings-font-change">
            {t("settings.fonts.change")}
          </span>
        </button>

        {customized && (
          <button
            type="button"
            className="sb-btn sb-btn-ghost settings-font-reset"
            onClick={() => {
              selection.current += 1;
              setPendingSelection(null);
              dropTokens([fontToken(slot), fontSourceToken(slot)]);
            }}
          >
            <RotateCcw size="1em" aria-hidden="true" />
            {t("settings.fonts.useTheme")}
          </button>
        )}

        {googleId !== null && installState.status === "error" && (
          <div className="settings-font-error" role="alert">
            <span>{t("settings.fonts.loadError")}</span>
            <button
              type="button"
              className="sb-btn sb-btn-ghost"
              onClick={() => {
                void ensureManagedFont(googleId).catch(reportError);
              }}
            >
              {t("settings.fonts.retry")}
            </button>
          </div>
        )}

        {open && (
          <div className="settings-font-picker">
            <div
              className="settings-font-source-tabs"
              role="group"
              aria-label={t("settings.fonts.source")}
            >
              {(["system", "google"] as const).map((value) => (
                <button
                  key={value}
                  type="button"
                  aria-pressed={sourceFilter === value}
                  className={sourceFilter === value ? "sb-active" : ""}
                  onClick={() => {
                    setSourceFilter(value);
                    setQuery("");
                  }}
                >
                  {t(`settings.fonts.${value}`)}
                </button>
              ))}
            </div>
            <label className="settings-font-search">
              <Search size="1em" aria-hidden="true" />
              <span className="sr-only">{t("settings.fonts.search")}</span>
              <input
                className="sb-input"
                type="search"
                value={query}
                placeholder={t("settings.fonts.search")}
                onChange={(event) => {
                  setQuery(event.target.value);
                }}
              />
            </label>

            <div className="settings-font-results" aria-live="polite">
              {loading && (
                <div className="settings-font-message">
                  <LoaderCircle className="settings-font-spinner" size="1em" />
                  {t("settings.fonts.loading")}
                </div>
              )}
              {!loading && listError && (
                <div className="settings-font-message" role="alert">
                  {t("settings.fonts.listError")}
                </div>
              )}
              {!loading && !listError && options.length === 0 && (
                <div className="settings-font-message">
                  {t("settings.fonts.empty")}
                </div>
              )}
              {!loading &&
                !listError &&
                options.map((option) => {
                  const active =
                    option.source === "google"
                      ? option.id === googleId
                      : source === "system" && option.family === currentName;
                  const installing =
                    option.source === "google" && option.id === pendingOptionId;
                  return (
                    <button
                      key={`${option.source}:${option.id}`}
                      type="button"
                      className="settings-font-option"
                      data-active={active ? "" : undefined}
                      disabled={installing}
                      onClick={() => {
                        choose(option);
                      }}
                    >
                      <span
                        className="settings-font-option-sample"
                        style={
                          option.source === "system" || option.cached
                            ? {
                                fontFamily: fontFamilyStack(
                                  option.family,
                                  slot,
                                  option.category,
                                ),
                              }
                            : undefined
                        }
                        aria-hidden="true"
                      >
                        {slot === "mono" ? "01" : "Aa"}
                      </span>
                      <span className="settings-font-option-copy">
                        <strong>{option.family}</strong>
                        <small>{categoryLabel(option.category)}</small>
                      </span>
                      {installing ? (
                        <LoaderCircle
                          className="settings-font-spinner"
                          size="1em"
                          aria-label={t("settings.fonts.installing")}
                        />
                      ) : active ? (
                        <Check
                          size="1em"
                          aria-label={t("settings.fonts.selected")}
                        />
                      ) : option.source === "google" && !option.cached ? (
                        <Download
                          size="1em"
                          aria-label={t("settings.fonts.download")}
                        />
                      ) : null}
                    </button>
                  );
                })}
            </div>

            {failedOption !== null && (
              <div className="settings-font-error" role="alert">
                <span>{t("settings.fonts.loadError")}</span>
                <button
                  type="button"
                  className="sb-btn sb-btn-ghost"
                  onClick={() => {
                    choose(failedOption);
                  }}
                >
                  {t("settings.fonts.retry")}
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </SettingRow>
  );
}

function firstFamily(stack: string): string {
  return (stack.split(",")[0] ?? "").trim().replace(/^(['"])(.*)\1$/, "$2");
}

function sourceLabel(
  source: string,
  status: "idle" | "loading" | "ready" | "error",
  systemAvailable: boolean | null,
): string {
  if (!source.startsWith("google:")) {
    return systemAvailable === false
      ? t("settings.fonts.unavailable")
      : t("settings.fonts.systemFont");
  }
  if (status === "loading") return t("settings.fonts.installing");
  if (status === "error") return t("settings.fonts.loadError");
  return t("settings.fonts.googleFont");
}

function categoryLabel(category: FontOption["category"]): string {
  return t(`settings.fonts.category.${category}`);
}
