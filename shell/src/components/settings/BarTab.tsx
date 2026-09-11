import { t } from "../../i18n/t";
import {
  clampMaxWidth,
  useSmabar,
  type BarPosition,
  type BarVariant,
  type BarWidth,
  type LayoutBehavior,
  type PopupPosition,
  type ZOrder,
  type ZoneKind,
} from "../../store/bar";
import {
  Choice,
  ChoiceGrid,
  SettingGroup,
  SettingReveal,
  SettingRow,
  SettingsSection,
  SliderRow,
  Switch,
} from "./controls";
import { BAR_DEFAULTS, BAR_TOKENS } from "./defaults";
import { SettingColumns } from "./layout";
import {
  BAR_PAD_Y_DEFAULT,
  BAR_PAD_Y_MAX,
  BAR_PAD_Y_MIN,
  MARGIN_MAX,
  MARGIN_MIN,
  MARGIN_STEP,
  SCALE_DEFAULT,
  SCALE_MAX,
  SCALE_MIN,
  clampMargin,
  tokenNumber,
  whitespaceTokens,
} from "./model";
import {
  BehaviorPictogram,
  PositionPictogram,
  PopupPositionPictogram,
  StackPictogram,
  VariantPictogram,
  WidthPictogram,
  ZonePictogram,
} from "./Pictograms";
import {
  setConfig,
  setConfigDebounced,
  setConfigsSequentially,
} from "./persist";
import { clearTokens, writeTokens } from "./tokens";
import { MonitorSetting } from "./MonitorSetting";
import { ReservationAccess } from "./ReservationAccess";

const POSITIONS: BarPosition[] = ["top", "bottom"];
const VARIANTS: BarVariant[] = ["split", "rows", "solo"];
const WIDTHS: BarWidth[] = ["full", "auto"];
const ZONES: ZoneKind[] = ["shortcuts", "plugins"];
const BEHAVIORS: LayoutBehavior[] = ["reserve", "float", "autohide"];
const Z_ORDERS: ZOrder[] = ["top", "bottom"];
const POPUP_POSITIONS: PopupPosition[] = [
  "top-left",
  "top-center",
  "top-right",
  "bottom-left",
  "bottom-center",
  "bottom-right",
];

const MAX_WIDTH_DEFAULT = 1_200;
const MAX_WIDTH_MIN = 400;
const MAX_WIDTH_MAX = 2_400;
const MAX_WIDTH_STEP = 100;

/** The bar itself: where it sits, how it is arranged, how big, how it behaves. */
export function BarTab() {
  const layout = useSmabar((state) => state.layout);
  const zOrder = useSmabar((state) => state.zOrder);
  const popups = useSmabar((state) => state.popups);
  const padY = useSmabar((state) =>
    tokenNumber(state.appearance.tokens, "--sb-bar-pad-y", BAR_PAD_Y_DEFAULT),
  );
  const scale = useSmabar((state) =>
    Math.round(
      tokenNumber(state.appearance.tokens, "--sb-scale", SCALE_DEFAULT / 100) *
        100,
    ),
  );
  const split = layout.variant === "split";
  const auto = layout.width === "auto";
  const capped = layout.maxWidth !== 0;

  const setLiveLayout = (patch: Partial<typeof layout>) => {
    const state = useSmabar.getState();
    state.setLayout({ ...state.layout, ...patch });
  };

  return (
    <SettingsSection
      title={t("settings.group.bar")}
      onReset={() =>
        setConfigsSequentially([...BAR_DEFAULTS, clearTokens(BAR_TOKENS)])
      }
    >
      <SettingGroup title={t("settings.bar.placement")}>
        <SettingRow
          label={t("settings.monitor.label")}
          description={t("settings.monitor.description")}
        >
          <MonitorSetting />
        </SettingRow>

        <SettingRow
          label={t("settings.layout.position")}
          description={t("settings.location.positionDescription")}
        >
          <ChoiceGrid label={t("settings.layout.position")}>
            {POSITIONS.map((position) => (
              <Choice
                key={position}
                label={t(`settings.pos.${position}`)}
                active={layout.position === position}
                onClick={() => {
                  setConfig("layout.position", position);
                }}
              >
                <PositionPictogram value={position} />
              </Choice>
            ))}
          </ChoiceGrid>
        </SettingRow>

        <SettingRow
          label={t("settings.layout.margin")}
          description={t("settings.location.marginDescription")}
        >
          <SliderRow
            label={t("settings.layout.margin")}
            min={MARGIN_MIN}
            max={MARGIN_MAX}
            step={MARGIN_STEP}
            value={clampMargin(layout.margin)}
            display={`${String(clampMargin(layout.margin))}px`}
            onChange={(value) => {
              setLiveLayout({ margin: value });
              setConfigDebounced("layout.margin", value);
            }}
          />
        </SettingRow>
      </SettingGroup>

      <SettingGroup title={t("settings.bar.arrangement")}>
        <SettingRow
          label={t("settings.layout.variant")}
          description={t("settings.location.variantDescription")}
        >
          <ChoiceGrid label={t("settings.layout.variant")}>
            {VARIANTS.map((variant) => (
              <Choice
                key={variant}
                label={t(`settings.variant.${variant}`)}
                active={layout.variant === variant}
                onClick={() => {
                  setConfig("layout.variant", variant);
                }}
              >
                <VariantPictogram value={variant} />
              </Choice>
            ))}
          </ChoiceGrid>
        </SettingRow>

        <SettingReveal visible={!split}>
          <SettingRow
            label={t("settings.layout.primaryZone")}
            description={t("settings.location.primaryZoneDescription")}
          >
            <ChoiceGrid label={t("settings.layout.primaryZone")}>
              {ZONES.map((zone) => (
                <Choice
                  key={zone}
                  label={t(`settings.zone.${zone}`)}
                  active={layout.primaryZone === zone}
                  disabled={split}
                  onClick={() => {
                    setConfig("layout.primaryZone", zone);
                  }}
                >
                  <ZonePictogram value={zone} />
                </Choice>
              ))}
            </ChoiceGrid>
          </SettingRow>
        </SettingReveal>

        <SettingRow
          label={t("settings.layout.width")}
          description={t("settings.location.widthDescription")}
        >
          <ChoiceGrid label={t("settings.layout.width")}>
            {WIDTHS.map((width) => (
              <Choice
                key={width}
                label={t(`settings.width.${width}`)}
                active={layout.width === width}
                onClick={() => {
                  setConfig("layout.width", width);
                }}
              >
                <WidthPictogram value={width} />
              </Choice>
            ))}
          </ChoiceGrid>
        </SettingRow>

        <SettingReveal visible={!auto}>
          <SettingRow
            label={t("settings.location.maxWidthEnabled")}
            description={t("settings.location.maxWidthDescription")}
            control={
              <Switch
                label={t("settings.location.maxWidthEnabled")}
                checked={capped}
                disabled={auto}
                onChange={(enabled) => {
                  setConfig("layout.maxWidth", enabled ? MAX_WIDTH_DEFAULT : 0);
                }}
              />
            }
          >
            {capped && (
              <SliderRow
                label={t("settings.location.maxWidth")}
                min={MAX_WIDTH_MIN}
                max={MAX_WIDTH_MAX}
                step={MAX_WIDTH_STEP}
                value={clampMaxWidth(layout.maxWidth, MAX_WIDTH_MAX)}
                display={`${String(clampMaxWidth(layout.maxWidth, MAX_WIDTH_MAX))}px`}
                disabled={auto}
                onChange={(value) => {
                  setLiveLayout({ maxWidth: value });
                  setConfigDebounced("layout.maxWidth", value);
                }}
              />
            )}
          </SettingRow>
        </SettingReveal>
      </SettingGroup>

      <SettingGroup title={t("settings.bar.window")}>
        <SettingRow
          label={t("settings.behavior.mode")}
          description={t("settings.behavior.modeDescription")}
        >
          <ChoiceGrid label={t("settings.behavior.mode")}>
            {BEHAVIORS.map((behavior) => (
              <Choice
                key={behavior}
                label={t(`settings.behavior.${behavior}`)}
                active={layout.behavior === behavior}
                onClick={() => {
                  setConfig("layout.behavior", behavior);
                }}
              >
                <BehaviorPictogram value={behavior} />
              </Choice>
            ))}
          </ChoiceGrid>
        </SettingRow>

        {layout.behavior === "reserve" && <ReservationAccess />}
        <SettingReveal visible={layout.behavior === "float"}>
          <SettingRow
            label={t("settings.behavior.zOrder")}
            description={t("settings.behavior.zOrderDescription")}
          >
            <ChoiceGrid label={t("settings.behavior.zOrder")}>
              {Z_ORDERS.map((value) => (
                <Choice
                  key={value}
                  label={t(`settings.zOrder.${value}`)}
                  active={zOrder === value}
                  onClick={() => {
                    setConfig("zOrder", value);
                  }}
                >
                  <StackPictogram value={value} />
                </Choice>
              ))}
            </ChoiceGrid>
          </SettingRow>
        </SettingReveal>

        <SettingRow
          label={t("settings.behavior.yieldToFullscreen")}
          description={t("settings.behavior.yieldToFullscreenDescription")}
          control={
            <Switch
              label={t("settings.behavior.yieldToFullscreen")}
              checked={layout.yieldToFullscreen}
              onChange={(checked) => {
                setConfig("layout.yieldToFullscreen", checked);
              }}
            />
          }
        />
      </SettingGroup>

      <SettingColumns>
        <SettingGroup title={t("settings.bar.size")}>
          <SettingRow
            label={t("settings.appearance.scale")}
            description={t("settings.appearance.scaleDescription")}
          >
            <SliderRow
              label={t("settings.appearance.scale")}
              min={SCALE_MIN}
              max={SCALE_MAX}
              step={5}
              value={scale}
              display={`${String(scale)}%`}
              onChange={(value) => {
                // Unitless: :root { font-size: calc(100% * var(--sb-scale)) }.
                writeTokens({ "--sb-scale": String(value / 100) });
              }}
            />
          </SettingRow>

          <SettingRow
            label={t("settings.appearance.whitespace")}
            description={t("settings.appearance.whitespaceDescription")}
          >
            <SliderRow
              label={t("settings.appearance.whitespace")}
              min={BAR_PAD_Y_MIN}
              max={BAR_PAD_Y_MAX}
              step={1}
              value={padY}
              display={`${String(padY)}px`}
              onChange={(value) => {
                writeTokens(whitespaceTokens(value));
              }}
            />
          </SettingRow>
        </SettingGroup>

        <SettingGroup title={t("settings.bar.notifications")}>
          <SettingRow
            label={t("settings.behavior.popups")}
            description={t("settings.behavior.popupsDescription")}
            control={
              <Switch
                label={t("settings.behavior.popups")}
                checked={popups.enabled}
                onChange={(enabled) => {
                  useSmabar.getState().setPopups({ ...popups, enabled });
                  setConfig("popups.enabled", enabled);
                }}
              />
            }
          />

          <SettingRow
            label={t("settings.behavior.popupPosition")}
            description={t("settings.behavior.popupPositionDescription")}
            disabledReason={
              popups.enabled ? undefined : t("settings.behavior.popupsOff")
            }
          >
            <ChoiceGrid
              label={t("settings.behavior.popupPosition")}
              layout="grid"
            >
              {POPUP_POSITIONS.map((value) => (
                <Choice
                  key={value}
                  label={t(`settings.popupPosition.${value}`)}
                  active={popups.position === value}
                  disabled={!popups.enabled}
                  onClick={() => {
                    setConfig("popups.position", value);
                  }}
                >
                  <PopupPositionPictogram value={value} />
                </Choice>
              ))}
            </ChoiceGrid>
          </SettingRow>
        </SettingGroup>
      </SettingColumns>
    </SettingsSection>
  );
}
