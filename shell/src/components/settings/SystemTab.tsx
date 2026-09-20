import { useEffect, useState } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import { useSmabar } from "../../store/bar";
import { AudioGroup } from "./AudioGroup";
import { NumberRow } from "./NumberRow";
import { AboutGroup } from "./AboutGroup";
import { SettingGroup, SettingRow, SettingsSection, Switch } from "./controls";
import { pageDefaults } from "./pageDefaults";
import { BarTab } from "./BarTab";
import { LanguageChoice } from "./LanguageChoice";
import { LegalDocuments } from "./LegalTab";
import { RenderingGroup, type RenderingStatus } from "./RenderingGroup";
import { RuntimeGroup } from "./RuntimeGroup";
import { UpdateGroup } from "./UpdateGroup";
import { useAutostart } from "./useAutostart";
import {
  setConfig,
  setConfigDebounced,
  setConfigsSequentially,
} from "./persist";

/** Ports below 1024 need privileges no desktop app should ask for. */
const PORT_MIN = 1024;
const PORT_MAX = 65535;

/** Whatever `get_system_settings` reports; absent until the call returns. */
interface SystemSettings {
  updateChannel: "app" | "store";
  languages: string[];
  mcp: { enabled: boolean; port: number };
  rendering: RenderingStatus | null;
}

/** Product information followed by system-facing settings. */
export function SystemTab({
  page = "general",
}: {
  page?: "general" | "advanced" | "about" | "audio" | "legal";
}) {
  const language = useSmabar((state) => state.language);
  const [system, setSystem] = useState<SystemSettings | null>(null);
  const autostart = useAutostart();
  const autostartRegistered =
    autostart.status?.state === "unavailable"
      ? null
      : (autostart.status?.registered ?? null);

  useEffect(() => {
    call<SystemSettings>("get_system_settings")
      .then(setSystem)
      .catch(reportError);
  }, []);

  // Optimistic: the MCP config is read once and never pushed back, so the
  // controls have to remember what was just set.
  const patchMcp = (patch: Partial<SystemSettings["mcp"]>) => {
    setSystem((previous) =>
      previous === null
        ? previous
        : { ...previous, mcp: { ...previous.mcp, ...patch } },
    );
  };

  const mcpOff = system?.mcp.enabled === false;
  const rendering = system?.rendering ?? null;

  return (
    <SettingsSection
      title={t(`settings.page.${page}`)}
      onReset={
        page === "about" || page === "legal" || page === "audio"
          ? undefined
          : async () => {
              await setConfigsSequentially(pageDefaults(page));
              if (
                page === "general" &&
                autostart.status?.state !== "unavailable"
              ) {
                await autostart.setEnabled(true);
              }
            }
      }
    >
      {page === "about" && (
        <>
          <AboutGroup />
          {system?.updateChannel === "app" && <UpdateGroup />}
        </>
      )}
      <>
        {page === "general" && (
          <SettingGroup title={t("settings.system.general")}>
            <SettingRow
              label={t("settings.system.autostart")}
              description={t("settings.system.autostartDescription")}
              disabledReason={
                autostart.status?.state === "unavailable"
                  ? t("settings.system.autostartUnavailable")
                  : undefined
              }
              control={
                <Switch
                  label={t("settings.system.autostart")}
                  checked={autostartRegistered ?? false}
                  disabled={autostart.busy || autostartRegistered === null}
                  onChange={(enabled) => {
                    void autostart.setEnabled(enabled);
                  }}
                />
              }
            >
              {autostart.failed && (
                <div className="settings-font-error" role="alert">
                  <span>{t("settings.system.autostartFailed")}</span>
                  <button
                    type="button"
                    className="sb-btn sb-btn-ghost"
                    disabled={autostart.busy}
                    onClick={() => {
                      void autostart.refresh();
                    }}
                  >
                    {t("settings.system.autostartRefresh")}
                  </button>
                </div>
              )}
            </SettingRow>
            <SettingRow
              label={t("settings.system.language")}
              description={t("settings.system.languageDescription")}
            >
              <LanguageChoice languages={system?.languages ?? [language]} />
            </SettingRow>
          </SettingGroup>
        )}
        {page === "advanced" && (
          <SettingGroup title={t("settings.system.agent")}>
            <SettingRow
              label={t("settings.system.mcp")}
              description={t("settings.system.mcpDescription")}
              control={
                <Switch
                  label={t("settings.system.mcp")}
                  checked={system?.mcp.enabled ?? false}
                  disabled={system === null}
                  onChange={(enabled) => {
                    patchMcp({ enabled });
                    setConfig("mcp.enabled", enabled);
                  }}
                />
              }
            />

            <SettingRow
              label={t("settings.system.mcpPort")}
              description={t("settings.system.mcpPortDescription")}
              disabledReason={mcpOff ? t("settings.system.mcpOff") : undefined}
            >
              <NumberRow
                label={t("settings.system.mcpPort")}
                min={PORT_MIN}
                max={PORT_MAX}
                value={system?.mcp.port ?? PORT_MIN}
                disabled={system === null || mcpOff}
                onChange={(port) => {
                  patchMcp({ port });
                  setConfigDebounced("mcp.port", port);
                }}
              />
            </SettingRow>
          </SettingGroup>
        )}
      </>

      {page === "advanced" && rendering !== null && (
        <RenderingGroup
          status={rendering}
          onModeChange={(mode) => {
            setSystem((previous) =>
              previous?.rendering == null
                ? previous
                : {
                    ...previous,
                    rendering: { ...previous.rendering, mode },
                  },
            );
          }}
        />
      )}

      {page === "advanced" && <RuntimeGroup />}

      {page === "audio" && (
        <>
          <BarTab page="notifications" />
          <AudioGroup />
        </>
      )}
      {/* Accepted at first start; kept here, folded, for whoever wants to reread. */}
      {page === "legal" && (
        <SettingGroup title={t("settings.group.legal")}>
          <LegalDocuments />
        </SettingGroup>
      )}
    </SettingsSection>
  );
}
