import { t } from "../../i18n/t";
import { Choice, ChoiceGrid, SettingGroup, SettingRow } from "./controls";
import { setConfig } from "./persist";

export type RenderingMode = "auto" | "native" | "software";

export interface RenderingStatus {
  mode: RenderingMode;
  startupMode: RenderingMode;
  applied: "native" | "pinned" | "software";
}

const APPLIED_KEYS = {
  native: "settings.system.renderingNativeActive",
  pinned: "settings.system.renderingPinnedActive",
  software: "settings.system.renderingSoftwareActive",
} as const;

const MODES: readonly { mode: RenderingMode; label: string }[] = [
  { mode: "auto", label: "settings.system.renderingAuto" },
  { mode: "native", label: "settings.system.renderingNative" },
  { mode: "software", label: "settings.system.renderingSoftware" },
];

/** Linux renderer selection; WebKit applies it on the next app start. */
export function RenderingGroup({
  status,
  onModeChange,
}: {
  status: RenderingStatus;
  onModeChange: (mode: RenderingMode) => void;
}) {
  return (
    <SettingGroup title={t("settings.system.rendering")}>
      <SettingRow
        label={t("settings.system.rendering")}
        description={t("settings.system.renderingDescription")}
      >
        <ChoiceGrid label={t("settings.system.rendering")}>
          {MODES.map(({ mode, label }) => (
            <Choice
              key={mode}
              label={t(label)}
              active={status.mode === mode}
              onClick={() => {
                onModeChange(mode);
                setConfig("rendering", mode);
              }}
            />
          ))}
        </ChoiceGrid>
        <p role="status" className="sb-faint">
          {t(APPLIED_KEYS[status.applied])}
        </p>
        {status.mode !== status.startupMode && (
          <p role="status" className="sb-warn">
            {t("settings.system.renderingRestart")}
          </p>
        )}
      </SettingRow>
    </SettingGroup>
  );
}
