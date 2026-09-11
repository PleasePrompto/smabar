import { t } from "../../i18n/t";
import { useSmabar, type LabelMode, type ZoneAlign } from "../../store/bar";
import {
  Choice,
  ChoiceGrid,
  SettingGroup,
  SettingRow,
  SettingsSection,
  SliderRow,
  Switch,
} from "./controls";
import { SHORTCUTS_DEFAULTS, SHORTCUTS_TOKENS } from "./defaults";
import { SettingColumns } from "./layout";
import {
  GAP_DEFAULT,
  GAP_MAX,
  GAP_MIN,
  ICON_SIZE_MAX,
  ICON_SIZE_MIN,
  ICON_SIZE_STEP,
  LABEL_SIZE_MAX,
  LABEL_SIZE_MIN,
  MAGNIFY_MAX,
  MAGNIFY_MIN,
  MAGNIFY_STEP,
  NEIGHBORS_MAX,
  NEIGHBORS_MIN,
  clampIconSize,
  clampLabelSize,
  clampMagnifyScale,
  clampNeighbors,
  pxToRem,
  tokenNumber,
} from "./model";
import { AlignPictogram, LabelsPictogram } from "./Pictograms";
import { ShortcutAdd } from "./ShortcutAdd";
import {
  setConfig,
  setConfigDebounced,
  setConfigsSequentially,
} from "./persist";
import { PinnedList } from "./PinnedList";
import { clearTokens, writeTokens } from "./tokens";

const LABEL_MODES: LabelMode[] = ["right", "below", "hidden"];
const ALIGNS: ZoneAlign[] = ["left", "center", "right"];

/** The pinned shortcuts: what is on the bar, and how the tiles look and react. */
export function ShortcutsTab() {
  const shortcuts = useSmabar((s) => s.shortcuts);
  const effects = useSmabar((s) => s.effects);
  const align = useSmabar((s) => s.appearance.shortcutAlign);
  const gap = useSmabar((s) =>
    tokenNumber(s.appearance.tokens, "--sb-shortcut-gap", GAP_DEFAULT),
  );
  const magnify = effects.hoverMagnify;
  const labelsHidden = shortcuts.labels === "hidden";
  const setMagnify = (patch: Partial<typeof magnify>) => {
    useSmabar.getState().setEffects({
      ...effects,
      hoverMagnify: { ...magnify, ...patch },
    });
  };
  const reset = async () => {
    await setConfigsSequentially([
      ...SHORTCUTS_DEFAULTS,
      clearTokens(SHORTCUTS_TOKENS),
    ]);
  };

  return (
    <SettingsSection
      title={t("settings.group.shortcuts")}
      onReset={reset}
      resetQuestion={t("settings.shortcuts.resetConfirm")}
    >
      <ShortcutAdd />

      <SettingGroup title={t("settings.shortcuts.pinned")}>
        <SettingRow
          label={t("settings.shortcuts.pinned")}
          description={t("settings.shortcuts.pinnedDescription")}
          wide
        >
          <PinnedList />
        </SettingRow>
      </SettingGroup>

      <SettingColumns>
        <SettingGroup title={t("settings.shortcuts.look")}>
          <SettingRow
            label={t("settings.shortcuts.labels")}
            description={t("settings.shortcuts.labelsDescription")}
          >
            <ChoiceGrid label={t("settings.shortcuts.labels")}>
              {LABEL_MODES.map((mode) => (
                <Choice
                  key={mode}
                  label={t(`settings.labels.${mode}`)}
                  active={shortcuts.labels === mode}
                  onClick={() => {
                    setConfig("shortcuts.labels", mode);
                  }}
                >
                  <LabelsPictogram value={mode} />
                </Choice>
              ))}
            </ChoiceGrid>
          </SettingRow>

          <SettingRow
            label={t("settings.shortcuts.iconSize")}
            description={t("settings.shortcuts.iconSizeDescription")}
          >
            <SliderRow
              label={t("settings.shortcuts.iconSize")}
              min={ICON_SIZE_MIN}
              max={ICON_SIZE_MAX}
              step={ICON_SIZE_STEP}
              value={clampIconSize(shortcuts.iconSize)}
              display={`${String(clampIconSize(shortcuts.iconSize))}px`}
              onChange={(value) => {
                // Live bar feedback via the store; the config write is debounced.
                const s = useSmabar.getState();
                s.setShortcuts({ ...s.shortcuts, iconSize: value });
                setConfigDebounced("shortcuts.iconSize", value);
              }}
            />
          </SettingRow>

          <SettingRow
            label={t("settings.shortcuts.labelSize")}
            description={t("settings.shortcuts.labelSizeDescription")}
            disabledReason={
              labelsHidden ? t("settings.shortcuts.labelSizeOff") : undefined
            }
          >
            <SliderRow
              label={t("settings.shortcuts.labelSize")}
              min={LABEL_SIZE_MIN}
              max={LABEL_SIZE_MAX}
              step={1}
              value={clampLabelSize(shortcuts.labelSize)}
              display={`${String(clampLabelSize(shortcuts.labelSize))}px`}
              disabled={labelsHidden}
              onChange={(value) => {
                const s = useSmabar.getState();
                s.setShortcuts({ ...s.shortcuts, labelSize: value });
                setConfigDebounced("shortcuts.labelSize", value);
              }}
            />
          </SettingRow>

          <SettingRow
            label={t("settings.appearance.shortcutGap")}
            description={t("settings.appearance.shortcutGapDescription")}
          >
            <SliderRow
              label={t("settings.appearance.shortcutGap")}
              min={GAP_MIN}
              max={GAP_MAX}
              step={1}
              value={gap}
              display={`${String(gap)}px`}
              onChange={(value) => {
                writeTokens({ "--sb-shortcut-gap": pxToRem(value) });
              }}
            />
          </SettingRow>

          <SettingRow
            label={t("settings.appearance.shortcutAlign")}
            description={t("settings.appearance.shortcutAlignDescription")}
          >
            <ChoiceGrid label={t("settings.appearance.shortcutAlign")}>
              {ALIGNS.map((value) => (
                <Choice
                  key={value}
                  label={t(`settings.align.${value}`)}
                  active={align === value}
                  onClick={() => {
                    setConfig("appearance.shortcutAlign", value);
                  }}
                >
                  <AlignPictogram value={value} />
                </Choice>
              ))}
            </ChoiceGrid>
          </SettingRow>
        </SettingGroup>

        <SettingGroup title={t("settings.shortcuts.hover")}>
          <SettingRow
            label={t("settings.effects.magnify")}
            description={t("settings.effects.magnifyDescription")}
            control={
              <Switch
                label={t("settings.effects.magnify")}
                checked={magnify.enabled}
                onChange={(enabled) => {
                  setMagnify({ enabled });
                  setConfig("effects.hoverMagnify.enabled", enabled);
                }}
              />
            }
          />

          <SettingRow
            label={t("settings.effects.scale")}
            description={t("settings.effects.scaleDescription")}
            disabledReason={
              magnify.enabled ? undefined : t("settings.effects.magnifyOff")
            }
          >
            <SliderRow
              label={t("settings.effects.scale")}
              min={MAGNIFY_MIN}
              max={MAGNIFY_MAX}
              step={MAGNIFY_STEP}
              value={clampMagnifyScale(magnify.scale)}
              display={clampMagnifyScale(magnify.scale).toFixed(2)}
              disabled={!magnify.enabled}
              onChange={(value) => {
                const scale = clampMagnifyScale(value);
                setMagnify({ scale });
                setConfigDebounced("effects.hoverMagnify.scale", scale);
              }}
            />
          </SettingRow>

          <SettingRow
            label={t("settings.effects.neighbors")}
            description={t("settings.effects.neighborsDescription")}
            disabledReason={
              magnify.enabled ? undefined : t("settings.effects.magnifyOff")
            }
          >
            <SliderRow
              label={t("settings.effects.neighbors")}
              min={NEIGHBORS_MIN}
              max={NEIGHBORS_MAX}
              step={1}
              value={clampNeighbors(magnify.neighbors)}
              display={String(clampNeighbors(magnify.neighbors))}
              disabled={!magnify.enabled}
              onChange={(value) => {
                const neighbors = clampNeighbors(value);
                setMagnify({ neighbors });
                setConfigDebounced("effects.hoverMagnify.neighbors", neighbors);
              }}
            />
          </SettingRow>
        </SettingGroup>
      </SettingColumns>
    </SettingsSection>
  );
}
