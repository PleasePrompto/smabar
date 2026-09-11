import { Check, Pipette, RotateCcw, TriangleAlert } from "lucide-react";
import { useId, useRef, useState, type KeyboardEvent } from "react";

import { t } from "../../i18n/t";
import { colorsEqual, colorToHex } from "../../theme/contrast";

export interface ColorPreset {
  color: string;
  theme: string;
}

export interface ColorContrast {
  ratio: number;
  warning?: string;
}

interface HexEdit {
  /** Effective colour when editing began; a newer external value supersedes it. */
  base: string;
  draft: string;
  showError: boolean;
}

function titleCase(name: string): string {
  return name.charAt(0).toUpperCase() + name.slice(1);
}

/**
 * A compact colour well followed by explicit source choices. The actual
 * native picker stays available, but its browser-dependent swatch is hidden:
 * unsupported modern CSS colours must never masquerade as a black selection.
 */
export function ColorControl({
  label,
  value,
  themeColor,
  themeName,
  resetLabel,
  showThemeName,
  presets,
  contrast,
  onReset,
  onChange,
}: {
  label: string;
  /** Current override; empty means the active theme owns the colour. */
  value: string;
  themeColor: string;
  themeName: string;
  /** "Theme" for direct colours, "Automatic" for derived text. */
  resetLabel: string;
  /** Direct colours name the active theme; derived text does not. */
  showThemeName: boolean;
  presets: readonly ColorPreset[];
  contrast?: ColorContrast;
  onReset: () => void;
  onChange: (color: string) => void;
}) {
  const nativePicker = useRef<HTMLInputElement>(null);
  const errorId = useId();
  const overridden = value.trim() !== "";
  const effective = overridden ? value : themeColor;
  const pickerHex = colorToHex(effective);
  const [edit, setEdit] = useState<HexEdit | null>(null);
  const activeEdit = edit?.base === effective ? edit : null;
  const draft = activeEdit?.draft ?? pickerHex ?? "";
  const showError = activeEdit?.showError ?? false;
  const validDraft = normalizeHexDraft(draft);

  const visiblePresets = presets.filter((preset) => preset.theme !== themeName);
  const selectedPreset = overridden
    ? visiblePresets.find((preset) => colorsEqual(preset.color, value))
    : undefined;
  const custom = overridden && selectedPreset === undefined;
  const source = !overridden
    ? resetLabel
    : selectedPreset === undefined
      ? t("settings.colors.customShort")
      : titleCase(selectedPreset.theme);
  const sourceKind = !overridden
    ? "automatic"
    : selectedPreset === undefined
      ? "custom"
      : "preset";

  return (
    <div className="settings-color-control">
      <div className="settings-color-summary" data-source={sourceKind}>
        <span
          className="settings-color-preview"
          style={{ background: effective }}
          aria-hidden="true"
        />
        <span className="settings-color-meta">
          <span className="settings-color-source">
            {source}
            {!overridden && showThemeName && themeName !== "" && (
              <span>{titleCase(themeName)}</span>
            )}
          </span>
          <code className="settings-color-value">{effective}</code>
        </span>
        <label className="settings-color-hex">
          <span>{t("settings.colors.hex")}</span>
          <input
            className="sb-input"
            value={draft}
            maxLength={7}
            inputMode="text"
            autoComplete="off"
            spellCheck={false}
            placeholder={t("settings.colors.hexPlaceholder")}
            aria-invalid={showError && validDraft === null ? true : undefined}
            aria-describedby={
              showError && validDraft === null ? errorId : undefined
            }
            onChange={(event) => {
              const next = event.target.value;
              const normalized = normalizeHexDraft(next);
              setEdit({ base: effective, draft: next, showError: false });
              if (normalized !== null) onChange(normalized);
            }}
            onBlur={() => {
              setEdit({
                base: effective,
                draft,
                showError: validDraft === null,
              });
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                if (validDraft === null) {
                  setEdit({ base: effective, draft, showError: true });
                } else onChange(validDraft);
              }
              if (event.key === "Escape") {
                setEdit(null);
              }
            }}
          />
        </label>
        {contrast !== undefined && (
          <span
            className={`settings-color-contrast ${
              contrast.warning === undefined ? "" : "is-warning"
            }`}
            aria-label={`${t("settings.colors.contrast")}: ${contrast.ratio.toFixed(2)}:1`}
          >
            {contrast.ratio.toFixed(1)}:1
          </span>
        )}
      </div>

      <div
        className="settings-color-toolbar"
        role="toolbar"
        aria-label={`${label}: ${t("settings.colors.options")}`}
      >
        <button
          type="button"
          data-color-option=""
          data-auto=""
          className={`settings-color-option ${overridden ? "" : "sb-active"}`}
          aria-label={`${label}: ${resetLabel}`}
          aria-pressed={!overridden}
          tabIndex={overridden ? -1 : 0}
          onKeyDown={moveToolbarFocus}
          onClick={onReset}
        >
          <RotateCcw size="1em" aria-hidden="true" />
          {resetLabel}
          {!overridden && (
            <Check className="settings-color-check" aria-hidden="true" />
          )}
        </button>

        {visiblePresets.map((preset) => {
          const active = selectedPreset?.color === preset.color;
          return (
            <button
              key={`${preset.theme}:${preset.color}`}
              type="button"
              data-color-option=""
              className={`settings-color-option ${active ? "sb-active" : ""}`}
              aria-label={`${label}: ${titleCase(preset.theme)} ${preset.color}`}
              aria-pressed={active}
              tabIndex={active ? 0 : -1}
              onKeyDown={moveToolbarFocus}
              onClick={() => {
                onChange(preset.color);
              }}
            >
              <span
                className="settings-color-option-swatch"
                style={{ background: preset.color }}
                aria-hidden="true"
              />
              <span>{titleCase(preset.theme)}</span>
              {active && (
                <Check className="settings-color-check" aria-hidden="true" />
              )}
            </button>
          );
        })}

        <button
          type="button"
          data-color-option=""
          className={`settings-color-option ${custom ? "sb-active" : ""}`}
          aria-label={`${label}: ${t("settings.colors.custom")}`}
          aria-pressed={custom}
          tabIndex={custom ? 0 : -1}
          onKeyDown={moveToolbarFocus}
          onClick={() => {
            nativePicker.current?.click();
          }}
        >
          <Pipette size="1em" aria-hidden="true" />
          {t("settings.colors.customShort")}
          {custom && (
            <Check className="settings-color-check" aria-hidden="true" />
          )}
        </button>
        <input
          ref={nativePicker}
          type="color"
          className="settings-color-native"
          value={colorToHex(effective) ?? "#ffffff"}
          tabIndex={-1}
          aria-label={`${label}: ${t("settings.colors.custom")}`}
          onChange={(event) => {
            onChange(event.target.value);
          }}
        />
      </div>

      {contrast?.warning !== undefined && (
        <p className="settings-color-warning" role="status">
          <TriangleAlert size="1em" aria-hidden="true" />
          {contrast.warning}
        </p>
      )}
      {showError && validDraft === null && (
        <p id={errorId} className="settings-color-error" role="alert">
          {t("settings.colors.hexInvalid")}
        </p>
      )}
    </div>
  );
}

/** Accept six hex digits and persist one canonical #rrggbb value. */
function normalizeHexDraft(draft: string): string | null {
  const match = /^#?([0-9a-f]{6})$/i.exec(draft.trim())?.[1];
  if (match === undefined) return null;
  return `#${match.toLowerCase()}`;
}

/** One tab stop per toolbar; arrow keys inspect choices without selecting. */
function moveToolbarFocus(event: KeyboardEvent<HTMLButtonElement>): void {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    event.currentTarget.click();
    return;
  }
  const toolbar = event.currentTarget.parentElement;
  if (toolbar === null) return;
  const options = [
    ...toolbar.querySelectorAll<HTMLButtonElement>("[data-color-option]"),
  ];
  const current = options.indexOf(event.currentTarget);
  if (current < 0 || options.length === 0) return;

  let next: number | undefined;
  if (event.key === "Home") next = 0;
  if (event.key === "End") next = options.length - 1;
  if (event.key === "ArrowRight" || event.key === "ArrowDown") {
    next = (current + 1) % options.length;
  }
  if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
    next = (current - 1 + options.length) % options.length;
  }
  if (next === undefined) return;
  const target = options[next];
  if (target === undefined) return;
  event.preventDefault();
  event.currentTarget.tabIndex = -1;
  target.tabIndex = 0;
  target.focus();
}
