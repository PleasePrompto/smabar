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
  SliderRow,
  Switch,
} from "./controls";
import { GAP_DEFAULT, GAP_MAX, GAP_MIN, pxToRem, tokenNumber } from "./model";
import { AlignPictogram, ChromePictogram } from "./Pictograms";
import { setConfig, setConfigDebounced } from "./persist";
import { writeTokens } from "./tokens";

const CHROME = ["card", "flat"] as const satisfies readonly TileChrome[];
const ALIGNS: ZoneAlign[] = ["left", "center", "right"];

export function PluginPresentation({
  behavior = false,
}: {
  behavior?: boolean;
}) {
  const effects = useSmabar((state) => state.effects);
  const tileChrome = useSmabar((state) => state.appearance.tileChrome);
  const pluginAlign = useSmabar((state) => state.appearance.pluginAlign);
  const pluginAccent = useSmabar((state) => state.appearance.pluginAccent);
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

  return behavior ? (
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
  ) : (
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
  );
}
