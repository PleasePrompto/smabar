import { Store } from "lucide-react";
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
import { DESIGN_TOKENS, PLUGINS_TOKENS, SHORTCUTS_TOKENS } from "./defaults";
import { pageDefaults } from "./pageDefaults";
import { PluginPresentation } from "./PluginPresentation";
import { ShortcutPresentation } from "./ShortcutPresentation";
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
import { clearTokens, writeTokens } from "./tokens";

const CHROME = ["card", "flat"] as const satisfies readonly TileChrome[];

/** Theme, colours, and the look of the bar's own surface. */
export function DesignTab({
  page = "themes",
}: {
  page?: "themes" | "colors" | "appearance";
}) {
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
  const [themes, setThemes] = useState<ThemeSummary[]>([]);
  const active = useSmabar((state) => state.theme);
  const layout = useSmabar((state) => state.layout);

  useEffect(() => {
    let disposed = false;
    call<ThemeSummary[]>("list_themes")
      .then((result) => {
        if (!disposed) setThemes(result);
      })
      .catch(reportError);
    return () => {
      disposed = true;
    };
  }, [active, layout]);

  return (
    <SettingsSection
      title={t(`settings.page.${page}`)}
      onReset={() =>
        setConfigsSequentially([
          ...pageDefaults(page),
          ...(page === "themes"
            ? []
            : [
                clearTokens(
                  page === "colors"
                    ? [...COLOR_TOKENS, ...FONT_TOKENS]
                    : [
                        ...DESIGN_TOKENS,
                        ...PLUGINS_TOKENS,
                        ...SHORTCUTS_TOKENS,
                      ],
                ),
              ]),
        ])
      }
    >
      {page === "themes" && (
        <>
          <ThemeManager themes={themes} onThemes={setThemes} />
          <SettingGroup title={t("settings.themes.communityTitle")}>
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
        </>
      )}
      {page === "colors" && (
        <>
          <SettingGroup title={t("settings.design.colors")}>
            <ColorSettings themes={themes} />
          </SettingGroup>

          <SettingGroup title={t("settings.design.typography")}>
            <FontSettings themes={themes} />
          </SettingGroup>
        </>
      )}
      {page === "appearance" && (
        <>
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
                    writeTokens({
                      "--sb-bar-border-width": `${String(value)}px`,
                    });
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
          <PluginPresentation />
          <ShortcutPresentation />
        </>
      )}
    </SettingsSection>
  );
}
