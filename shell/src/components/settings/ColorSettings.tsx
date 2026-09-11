import { t } from "../../i18n/t";
import { useSmabar, type ThemeSummary } from "../../store/bar";
import {
  MIN_CONTRAST,
  colorsEqual,
  contrastTextOnAll,
  minimumContrast,
  readableTextOn,
} from "../../theme/contrast";
import { ColorControl, type ColorPreset } from "./ColorControl";
import { SettingRow } from "./controls";
import {
  accentPrimaryTokens,
  accentSecondaryTokens,
  surfaceTokens,
  textTokens,
} from "./model";
import { dropTokens, writeTokens } from "./tokens";

/** Theme-derived presets are deduplicated by colour, not CSS spelling. */
function presetsFrom(
  themes: readonly ThemeSummary[],
  pick: (colors: ThemeSummary["colors"]) => string,
): ColorPreset[] {
  const presets: ColorPreset[] = [];
  for (const theme of themes) {
    const color = pick(theme.colors).trim();
    if (
      color !== "" &&
      !presets.some((preset) => colorsEqual(preset.color, color))
    ) {
      presets.push({ color, theme: theme.name });
    }
  }
  return presets;
}

/** Four semantic colour overrides layered independently over the theme. */
export function ColorSettings({ themes }: { themes: readonly ThemeSummary[] }) {
  const activeName = useSmabar((state) => state.theme);
  const accentOverride = useSmabar(
    (state) => state.appearance.tokens["--sb-accent"],
  );
  const accent2Override = useSmabar(
    (state) => state.appearance.tokens["--sb-accent-2"],
  );
  const surfaceOverride = useSmabar(
    (state) => state.appearance.tokens["--sb-bar-bg"],
  );
  const textOverride = useSmabar(
    (state) => state.appearance.tokens["--sb-text"],
  );
  const themeColors = themes.find((theme) => theme.name === activeName)?.colors;

  const accent =
    accentOverride === undefined || accentOverride === ""
      ? (themeColors?.accent ?? "")
      : accentOverride;
  const accent2 =
    accent2Override === undefined || accent2Override === ""
      ? (themeColors?.accent2 ?? "")
      : accent2Override;
  const surface =
    surfaceOverride === undefined || surfaceOverride === ""
      ? (themeColors?.surface ?? "")
      : surfaceOverride;
  const themeText = themeColors?.text ?? "";
  const automaticText = readableTextOn(surface, themeText) ?? themeText;
  const text =
    textOverride === undefined || textOverride === ""
      ? automaticText
      : textOverride;

  const accentForeground = contrastTextOnAll(
    [accent, accent2].filter((color) => color !== ""),
  );
  const accentRatio =
    accentForeground === null
      ? null
      : minimumContrast([accent, accent2], accentForeground);
  const textRatio = minimumContrast([surface], text);

  const resetPrimaryAccent = () => {
    const keys = ["--sb-accent", "--sb-accent-glow"];
    // The standard gradient belongs to whichever endpoint remains custom.
    if (accent2Override === undefined) {
      keys.push("--sb-accent-gradient");
    }
    dropTokens(keys);
  };
  const resetSecondaryAccent = () => {
    const keys = ["--sb-accent-2"];
    if (accentOverride === undefined) {
      keys.push("--sb-accent-gradient");
    }
    dropTokens(keys);
  };

  return (
    <>
      <SettingRow
        label={t("settings.colors.accent")}
        description={t("settings.colors.accentDescription")}
        wide
      >
        <ColorControl
          label={t("settings.colors.accent")}
          value={accentOverride ?? ""}
          themeColor={themeColors?.accent ?? ""}
          themeName={activeName}
          resetLabel={t("settings.colors.fromTheme")}
          showThemeName
          presets={presetsFrom(themes, (colors) => colors.accent)}
          onReset={resetPrimaryAccent}
          onChange={(color) => {
            writeTokens(accentPrimaryTokens(color));
          }}
        />
      </SettingRow>

      <SettingRow
        label={t("settings.colors.accent2")}
        description={t("settings.colors.accent2Description")}
        wide
      >
        <ColorControl
          label={t("settings.colors.accent2")}
          value={accent2Override ?? ""}
          themeColor={themeColors?.accent2 ?? ""}
          themeName={activeName}
          resetLabel={t("settings.colors.fromTheme")}
          showThemeName
          presets={presetsFrom(themes, (colors) => colors.accent2)}
          contrast={
            accentRatio === null
              ? undefined
              : {
                  ratio: accentRatio,
                  warning:
                    accentRatio < MIN_CONTRAST
                      ? t("settings.colors.accentContrastWarning")
                      : undefined,
                }
          }
          onReset={resetSecondaryAccent}
          onChange={(color) => {
            writeTokens(accentSecondaryTokens(color));
          }}
        />
      </SettingRow>

      <SettingRow
        label={t("settings.colors.surface")}
        description={t("settings.colors.surfaceDescription")}
        wide
      >
        <ColorControl
          label={t("settings.colors.surface")}
          value={surfaceOverride ?? ""}
          themeColor={themeColors?.surface ?? ""}
          themeName={activeName}
          resetLabel={t("settings.colors.fromTheme")}
          showThemeName
          presets={presetsFrom(themes, (colors) => colors.surface)}
          onReset={() => {
            dropTokens(Object.keys(surfaceTokens("")));
          }}
          onChange={(color) => {
            writeTokens(surfaceTokens(color));
          }}
        />
      </SettingRow>

      <SettingRow
        label={t("settings.colors.text")}
        description={t("settings.colors.textDescription")}
        wide
      >
        <ColorControl
          label={t("settings.colors.text")}
          value={textOverride ?? ""}
          themeColor={automaticText}
          themeName={activeName}
          resetLabel={t("settings.colors.automatic")}
          showThemeName={false}
          presets={presetsFrom(themes, (colors) => colors.text)}
          contrast={
            textRatio === null
              ? undefined
              : {
                  ratio: textRatio,
                  warning:
                    (textOverride ?? "") !== "" && textRatio < MIN_CONTRAST
                      ? t("settings.colors.textContrastWarning")
                      : undefined,
                }
          }
          onReset={() => {
            dropTokens(Object.keys(textTokens("")));
          }}
          onChange={(color) => {
            writeTokens(textTokens(color));
          }}
        />
      </SettingRow>
    </>
  );
}
