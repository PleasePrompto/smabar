import { Store } from "lucide-react";

import { t } from "../../i18n/t";
import {
  clampHoverPeekDelay,
  HOVER_PEEK_DELAY_MAX_MS,
  HOVER_PEEK_DELAY_MIN_MS,
  useSmabar,
  type TileChrome,
  type ZoneAlign,
} from "../../store/bar";
import {
  Choice,
  ChoiceGrid,
  SettingGroup,
  SettingRow,
  SettingsSection,
  SliderRow,
  Switch,
} from "./controls";
import { PLUGINS_DEFAULTS, PLUGINS_TOKENS } from "./defaults";
import { SettingColumns } from "./layout";
import { GAP_DEFAULT, GAP_MAX, GAP_MIN, pxToRem, tokenNumber } from "./model";
import { AlignPictogram, ChromePictogram } from "./Pictograms";
import {
  setConfig,
  setConfigDebounced,
  setConfigsSequentially,
} from "./persist";
import { PluginCards } from "./PluginCards";
import { RuntimeGroup } from "./RuntimeGroup";
import { usePluginManagement } from "./usePluginManagement";
import { clearTokens, writeTokens } from "./tokens";
import { PluginList } from "./PluginList";

const CHROME = ["card", "flat"] as const satisfies readonly TileChrome[];
const ALIGNS: ZoneAlign[] = ["left", "center", "right"];

/** Bar-wide presentation, a compact sort list, and installed plugin cards. */
export function PluginsTab() {
  const management = usePluginManagement();
  const setGroup = useSmabar((state) => state.setSettingsGroup);
  const effects = useSmabar((state) => state.effects);
  const tileChrome = useSmabar((state) => state.appearance.tileChrome);
  const pluginAlign = useSmabar((state) => state.appearance.pluginAlign);
  const pluginAccent = useSmabar((state) => state.appearance.pluginAccent);
  const runtime = useSmabar((state) => state.runtimeStatus);
  const gap = useSmabar((state) =>
    tokenNumber(state.appearance.tokens, "--sb-tile-gap", GAP_DEFAULT),
  );
  const peek = effects.hoverPeek;

  const setPeek = (patch: Partial<typeof peek>) => {
    useSmabar.getState().setEffects({
      ...effects,
      hoverPeek: { ...peek, ...patch },
    });
  };

  return (
    <SettingsSection
      title={t("settings.group.plugins")}
      onReset={() =>
        setConfigsSequentially([
          ...PLUGINS_DEFAULTS,
          clearTokens(PLUGINS_TOKENS),
        ])
      }
    >
      {runtime !== null &&
        (runtime.state === "installing" || runtime.state === "failed") && (
          // Python plugins wait for the runtime: its download or failure
          // (with Retry) belongs where the waiting cards are.
          <RuntimeGroup />
        )}
      <SettingGroup title={t("settings.plugins.installed")} updateKey="plugin">
        <SettingRow
          label={t("settings.plugins.installed")}
          description={t("settings.plugins.listDescription")}
          wide
        >
          <PluginList management={management} />
          {management.loadFailed ? (
            <div className="settings-plugin-error" role="alert">
              <span>{t("settings.plugins.loadFailed")}</span>
              <button
                type="button"
                className="sb-btn sb-btn-ghost"
                onClick={() => {
                  void management.refresh();
                }}
              >
                {t("settings.plugins.retry")}
              </button>
            </div>
          ) : (
            management.installed === null && (
              <p className="settings-help">{t("settings.plugins.loading")}</p>
            )
          )}
        </SettingRow>
        <SettingRow
          label={t("settings.store.title")}
          description={t("settings.plugins.storeTeaser")}
          control={
            <button
              type="button"
              className="sb-btn"
              onClick={() => {
                setGroup("plugins/store");
              }}
            >
              <Store size="1em" aria-hidden="true" />
              {t("settings.plugins.storeTeaserAction")}
            </button>
          }
        />
      </SettingGroup>

      <SettingColumns>
        <SettingGroup title={t("settings.plugins.look")}>
          <SettingRow
            label={t("settings.appearance.tileChrome")}
            description={t("settings.appearance.tileChromeDescription")}
          >
            <ChoiceGrid label={t("settings.appearance.tileChrome")}>
              {CHROME.map((value) => (
                <Choice
                  key={value}
                  label={t(`settings.appearance.tileChrome.${value}`)}
                  active={tileChrome === value}
                  onClick={() => {
                    setConfig("appearance.tileChrome", value);
                  }}
                >
                  <ChromePictogram value={value} />
                </Choice>
              ))}
            </ChoiceGrid>
          </SettingRow>

          <SettingRow
            label={t("settings.appearance.pluginAccent")}
            description={t("settings.appearance.pluginAccentDescription")}
            control={
              <Switch
                label={t("settings.appearance.pluginAccent")}
                checked={pluginAccent === "plugin"}
                onChange={(enabled) => {
                  setConfig(
                    "appearance.pluginAccent",
                    enabled ? "plugin" : "theme",
                  );
                }}
              />
            }
          />

          <SettingRow
            label={t("settings.appearance.tileGap")}
            description={t("settings.appearance.tileGapDescription")}
          >
            <SliderRow
              label={t("settings.appearance.tileGap")}
              min={GAP_MIN}
              max={GAP_MAX}
              step={1}
              value={gap}
              display={`${String(gap)}px`}
              onChange={(value) => {
                writeTokens({ "--sb-tile-gap": pxToRem(value) });
              }}
            />
          </SettingRow>

          <SettingRow
            label={t("settings.appearance.pluginAlign")}
            description={t("settings.appearance.pluginAlignDescription")}
          >
            <ChoiceGrid label={t("settings.appearance.pluginAlign")}>
              {ALIGNS.map((value) => (
                <Choice
                  key={value}
                  label={t(`settings.align.${value}`)}
                  active={pluginAlign === value}
                  onClick={() => {
                    setConfig("appearance.pluginAlign", value);
                  }}
                >
                  <AlignPictogram value={value} />
                </Choice>
              ))}
            </ChoiceGrid>
          </SettingRow>
        </SettingGroup>

        <SettingGroup title={t("settings.plugins.hover")}>
          <SettingRow
            label={t("settings.behavior.hoverPeek")}
            description={t("settings.behavior.hoverPeekDescription")}
            control={
              <Switch
                label={t("settings.behavior.hoverPeek")}
                checked={peek.enabled}
                onChange={(enabled) => {
                  setPeek({ enabled });
                  setConfig("effects.hoverPeek.enabled", enabled);
                }}
              />
            }
          />

          <SettingRow
            label={t("settings.behavior.hoverPeekDelay")}
            description={t("settings.behavior.hoverPeekDelayDescription")}
            disabledReason={
              peek.enabled ? undefined : t("settings.behavior.hoverPeekOff")
            }
          >
            <SliderRow
              label={t("settings.behavior.hoverPeekDelay")}
              min={HOVER_PEEK_DELAY_MIN_MS}
              max={HOVER_PEEK_DELAY_MAX_MS}
              step={50}
              value={clampHoverPeekDelay(peek.delayMs)}
              display={`${String(clampHoverPeekDelay(peek.delayMs))}ms`}
              disabled={!peek.enabled}
              onChange={(delayMs) => {
                setPeek({ delayMs });
                setConfigDebounced("effects.hoverPeek.delayMs", delayMs);
              }}
            />
          </SettingRow>
        </SettingGroup>
      </SettingColumns>

      <PluginCards management={management} />
    </SettingsSection>
  );
}
