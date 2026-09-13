import { RotateCcw, Store } from "lucide-react";
import { useEffect, useState } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import { useSmabar, type ThemeSummary, type TileChrome } from "../../store/bar";
import { FONT_TOKENS } from "../../theme/fonts";
import { ColorSettings } from "./ColorSettings";
import { FontSettings } from "./FontSettings";
import {
  Choice,
  ChoiceGrid,
  SettingGroup,
  SettingReveal,
  SettingRow,
  SettingsSection,
  SliderRow,
} from "./controls";
import { DESIGN_DEFAULTS, DESIGN_TOKENS } from "./defaults";
import {
  BAR_BORDER_DEFAULT,
  BAR_BORDER_MAX,
  BAR_BORDER_MIN,
  BAR_OPACITY_DEFAULT,
  BAR_OPACITY_MAX,
  BAR_OPACITY_MIN,
  BAR_RADIUS_DEFAULT,
  BAR_RADIUS_MAX,
  BAR_RADIUS_MIN,
  COLOR_TOKENS,
  FLYOUT_OPACITY_DEFAULT,
  pxToRem,
  tokenNumber,
} from "./model";
import { BarChromePictogram } from "./Pictograms";
import { setConfig, setConfigsSequentially } from "./persist";
import { ThemeManager } from "./ThemeManager";
import { UpdateDot } from "./UpdateBadge";
import { themeDisplayName } from "./model";
import { clearTokens, dropTokens, writeTokens } from "./tokens";

const CHROME = ["card", "flat"] as const satisfies readonly TileChrome[];

/**
 * Mini bar mock: surface strip with accent dots and a text line. Sizing and
 * layout come from `.settings-swatch` in settings.css — utility classes lose
 * against that un-layered file, so only colors are set here. Every color,
 * the hairline included, is the PREVIEWED theme's, never the active one.
 */
function ThemeSwatch({ colors }: { colors: ThemeSummary["colors"] }) {
  return (
    <span
      aria-hidden="true"
      className="settings-swatch"
      style={{
        background: colors.surface,
        borderColor: `color-mix(in srgb, ${colors.text} 22%, transparent)`,
      }}
    >
      <span style={{ background: colors.accent }} />
      <span style={{ background: colors.accent2 }} />
      <span style={{ background: colors.text }} />
    </span>
  );
}

/** Theme, colours, and the look of the bar's own surface. */
export function DesignTab() {
  const active = useSmabar((state) => state.theme);
  const updates = useSmabar((state) => state.communityUpdates);
  const setGroup = useSmabar((state) => state.setSettingsGroup);
  const barChrome = useSmabar((state) => state.appearance.barChrome);
  const radius = useSmabar((state) =>
    tokenNumber(state.appearance.tokens, "--sb-bar-radius", BAR_RADIUS_DEFAULT),
  );
  const barBorder = useSmabar((state) =>
    tokenNumber(
      state.appearance.tokens,
      "--sb-bar-border-width",
      BAR_BORDER_DEFAULT,
    ),
  );
  const opacity = useSmabar((state) =>
    tokenNumber(
      state.appearance.tokens,
      "--sb-bar-opacity",
      BAR_OPACITY_DEFAULT,
    ),
  );
  const flyoutOpacity = useSmabar((state) =>
    tokenNumber(
      state.appearance.tokens,
      "--sb-flyout-opacity",
      FLYOUT_OPACITY_DEFAULT,
    ),
  );
  const customized = useSmabar((state) =>
    [...COLOR_TOKENS, ...FONT_TOKENS].some(
      (key) => key in state.appearance.tokens,
    ),
  );
  const [themes, setThemes] = useState<ThemeSummary[]>([]);
  // Overrides win over the theme (theme/apply.ts), so the theme tile alone
  // would keep claiming a look the bar no longer has.

  useEffect(() => {
    call<ThemeSummary[]>("list_themes").then(setThemes).catch(reportError);
  }, []);

  return (
    <SettingsSection
      title={t("settings.group.design")}
      onReset={() =>
        setConfigsSequentially([
          ...DESIGN_DEFAULTS,
          clearTokens([...DESIGN_TOKENS, ...COLOR_TOKENS, ...FONT_TOKENS]),
        ])
      }
    >
      <SettingGroup title={t("settings.design.theme")} updateKey="theme">
        <SettingRow
          label={t("settings.appearance.theme")}
          description={t("settings.appearance.themeDescription")}
          wide
        >
          <ChoiceGrid label={t("settings.appearance.theme")}>
            {themes.map((theme) => (
              <Choice
                key={theme.name}
                label={themeDisplayName(theme)}
                active={theme.name === active}
                onClick={() => {
                  setConfig("theme", theme.name);
                }}
              >
                <ThemeSwatch colors={theme.colors} />
                {updates.some(
                  (entry) => entry.kind === "theme" && entry.id === theme.name,
                ) && <UpdateDot />}
              </Choice>
            ))}
          </ChoiceGrid>
          {themes.length === 0 && (
            <div className="sb-faint">{t("settings.themes.empty")}</div>
          )}
          {customized && (
            <div
              className="sb-inline"
              style={{ marginTop: "var(--sb-space-xs)" }}
            >
              <span className="sb-badge sb-badge-warn">
                {t("settings.design.customized")}
              </span>
              <button
                className="sb-btn sb-btn-ghost sb-push"
                onClick={() => {
                  dropTokens([...COLOR_TOKENS, ...FONT_TOKENS]);
                }}
              >
                <RotateCcw size="1em" aria-hidden="true" />
                {t("settings.design.resetOverrides")}
              </button>
            </div>
          )}
        </SettingRow>
        <SettingRow
          label={t("settings.themes.communityTitle")}
          description={t("settings.themes.storeTeaser")}
          control={
            <button
              type="button"
              className="sb-btn"
              onClick={() => {
                setGroup("design/themes");
              }}
            >
              <Store size="1em" aria-hidden="true" />
              {t("settings.themes.storeTeaserAction")}
            </button>
          }
        />
      </SettingGroup>

      <ThemeManager themes={themes} onThemes={setThemes} />

      <SettingGroup title={t("settings.design.colors")}>
        <ColorSettings themes={themes} />
      </SettingGroup>

      <SettingGroup title={t("settings.design.typography")}>
        <FontSettings themes={themes} />
      </SettingGroup>

      <SettingGroup title={t("settings.design.surface")}>
        <SettingRow
          label={t("settings.appearance.barChrome")}
          description={t("settings.appearance.barChromeDescription")}
        >
          <ChoiceGrid label={t("settings.appearance.barChrome")}>
            {CHROME.map((value) => (
              <Choice
                key={value}
                label={t(`settings.appearance.barChrome.${value}`)}
                active={barChrome === value}
                onClick={() => {
                  setConfig("appearance.barChrome", value);
                }}
              >
                <BarChromePictogram value={value} />
              </Choice>
            ))}
          </ChoiceGrid>
        </SettingRow>

        <SettingReveal visible={barChrome === "card"}>
          <SettingRow
            label={t("settings.appearance.barBorderWidth")}
            description={t("settings.appearance.barBorderWidthDescription")}
          >
            <SliderRow
              label={t("settings.appearance.barBorderWidth")}
              min={BAR_BORDER_MIN}
              max={BAR_BORDER_MAX}
              step={1}
              value={barBorder}
              display={`${String(barBorder)}px`}
              onChange={(value) => {
                writeTokens({ "--sb-bar-border-width": `${String(value)}px` });
              }}
            />
          </SettingRow>
        </SettingReveal>

        <SettingRow
          label={t("settings.appearance.barRadius")}
          description={t("settings.appearance.barRadiusDescription")}
        >
          <SliderRow
            label={t("settings.appearance.barRadius")}
            min={BAR_RADIUS_MIN}
            max={BAR_RADIUS_MAX}
            step={2}
            value={radius}
            display={`${String(radius)}px`}
            onChange={(value) => {
              writeTokens({ "--sb-bar-radius": pxToRem(value) });
            }}
          />
        </SettingRow>

        <SettingRow
          label={t("settings.appearance.barOpacity")}
          description={t("settings.appearance.barOpacityDescription")}
        >
          <SliderRow
            label={t("settings.appearance.barOpacity")}
            min={BAR_OPACITY_MIN}
            max={BAR_OPACITY_MAX}
            step={5}
            value={opacity}
            display={`${String(opacity)}%`}
            onChange={(value) => {
              writeTokens({ "--sb-bar-opacity": `${String(value)}%` });
            }}
          />
        </SettingRow>

        <SettingRow
          label={t("settings.appearance.flyoutOpacity")}
          description={t("settings.appearance.flyoutOpacityDescription")}
        >
          <SliderRow
            label={t("settings.appearance.flyoutOpacity")}
            min={BAR_OPACITY_MIN}
            max={BAR_OPACITY_MAX}
            step={5}
            value={flyoutOpacity}
            display={`${String(flyoutOpacity)}%`}
            onChange={(value) => {
              writeTokens({ "--sb-flyout-opacity": `${String(value)}%` });
            }}
          />
        </SettingRow>
      </SettingGroup>
    </SettingsSection>
  );
}
