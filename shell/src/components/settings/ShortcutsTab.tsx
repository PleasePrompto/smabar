import { t } from "../../i18n/t";
import { SettingGroup, SettingRow, SettingsSection } from "./controls";
import { ShortcutAdd } from "./ShortcutAdd";
import { PinnedList } from "./PinnedList";
import { setConfigsSequentially } from "./persist";

export function ShortcutsTab() {
  return (
    <SettingsSection
      title={t("settings.group.shortcuts")}
      onReset={() =>
        setConfigsSequentially([{ path: "shortcuts.pinned", value: [] }])
      }
      resetQuestion={t("settings.shortcuts.resetConfirm")}
    >
      <SettingGroup title={t("settings.shortcuts.pinned")}>
        <SettingRow
          label={t("settings.shortcuts.pinned")}
          description={t("settings.shortcuts.pinnedDescription")}
          wide
        >
          <PinnedList />
        </SettingRow>
      </SettingGroup>
      <ShortcutAdd />
    </SettingsSection>
  );
}
