import {
  Blocks,
  LayoutPanelTop,
  Link2,
  MonitorCog,
  ScrollText,
  X,
} from "lucide-react";
import { useEffect, useRef, type CSSProperties } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../../i18n/t";
import { closeCurrentSurface } from "../../ipc/surface";
import { reportError } from "../../ipc/log";
import { useSmabar } from "../../store/bar";
import { ResizeEdges } from "./ResizeEdges";
import {
  groupOf,
  pageOf,
  pagesOf,
  resolveSettingsPage,
  SETTINGS_GROUPS,
} from "./settingsPages";
import { StorePage } from "./StorePage";
import { LegalTab } from "./LegalTab";
import { UpdateDot } from "./UpdateBadge";
import { hasAppUpdate } from "../../ipc/updateSync";
import { usePluginManagement } from "./usePluginManagement";
import { SettingsBody } from "./SettingsBody";
import { SettingsFooter } from "./SettingsFooter";

const ICONS = {
  bar: LayoutPanelTop,
  shortcuts: Link2,
  plugins: Blocks,
  system: MonitorCog,
};

export function SettingsPanel({ preview = false }: { preview?: boolean }) {
  const requested = useSmabar((state) => state.settingsGroup);
  const setGroup = useSmabar((state) => state.setSettingsGroup);
  const updates = useSmabar((state) => state.communityUpdates);
  const appUpdate = useSmabar(hasAppUpdate);
  const deactivated = useSmabar((state) => state.pluginsDeactivated);
  const settingsWindow = useSmabar((state) =>
    preview ? state.settingsWindow : null,
  );
  const gated = useSmabar((state) => state.legalRequired);
  const management = usePluginManagement(!gated);
  const selected = resolveSettingsPage(requested);
  const current = groupOf(selected);
  const page = pageOf(selected);
  const closeRef = useRef<HTMLButtonElement>(null);
  const native = !preview && "__TAURI_INTERNALS__" in window;
  const count = (id: string) => {
    if (id === "system" || id === "system/about") return appUpdate ? 1 : 0;
    if (["bar", "bar/themes", "bar/community"].includes(id))
      return updates.filter((entry) => entry.kind === "theme").length;
    if (id === "plugins" || id === "plugins/store")
      return updates.filter((entry) => entry.kind === "plugin").length;
    return updates.filter(
      (entry) => id === `plugins/detail/${entry.id}` && entry.kind === "plugin",
    ).length;
  };
  const badge = (id: string) =>
    count(id) > 0 && (
      <UpdateDot
        label={
          id.startsWith("system")
            ? t("settings.update.badge")
            : t("settings.store.badge").replace("{count}", String(count(id)))
        }
      />
    );
  useEffect(() => {
    closeRef.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      event.preventDefault();
      void closeCurrentSurface().catch(reportError);
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, []);
  const previewStyle: CSSProperties | undefined =
    settingsWindow === null
      ? undefined
      : {
          left: "50%",
          top: "50%",
          width: Math.min(settingsWindow.width, window.innerWidth - 8),
          height: Math.min(settingsWindow.height, window.innerHeight - 8),
          transform: "translate(-50%, -50%)",
        };
  return (
    <div
      className={`sb-root sb-flush settings-panel ${preview ? "fixed" : "absolute inset-0"}`}
      style={previewStyle}
      data-capture="settings"
      role="dialog"
      aria-labelledby="settings-title"
      onClick={(event) => {
        event.stopPropagation();
      }}
      onAuxClick={(event) => {
        event.stopPropagation();
      }}
    >
      <div className="settings-window settings-panel-surface text-[color:var(--sb-text)]">
        <div
          className="sb-header settings-panel-header touch-none select-none cursor-grab active:cursor-grabbing"
          onMouseDown={(event) => {
            if (
              native &&
              event.button === 0 &&
              event.target instanceof Element &&
              event.target.closest("button") === null
            )
              void getCurrentWindow().startDragging().catch(reportError);
          }}
        >
          <div className="settings-title">
            <span className="settings-brand-mark" aria-hidden="true" />
            <h1 id="settings-title" className="sb-title">
              {t("settings.title")}
            </h1>
          </div>
          <button
            ref={closeRef}
            type="button"
            className="sb-btn sb-btn-ghost sb-btn-icon"
            aria-label={t("settings.close")}
            onClick={() => {
              void closeCurrentSurface().catch(reportError);
            }}
          >
            <X size="1em" />
          </button>
        </div>
        <div className="settings-workspace">
          <div className="settings-sidebar">
            <nav
              className="settings-group-nav"
              aria-label={t("settings.groups")}
            >
              {gated ? (
                <div className="settings-nav-section">
                  <button
                    type="button"
                    className="sb-active"
                    aria-label={t("settings.group.legal")}
                    aria-current="page"
                  >
                    <ScrollText size="1em" />
                    {t("settings.group.legal")}
                  </button>
                </div>
              ) : (
                SETTINGS_GROUPS.map((id) => {
                  const Icon = ICONS[id];
                  const entries = pagesOf(id);
                  return (
                    <div key={id} className="settings-nav-section">
                      <button
                        id={`settings-tab-${id}`}
                        type="button"
                        aria-label={t(`settings.group.${id}`)}
                        aria-controls="settings-content"
                        aria-current={current === id ? "page" : undefined}
                        className={current === id ? "sb-active" : undefined}
                        onClick={() => {
                          setGroup(entries[0]?.id ?? id);
                        }}
                      >
                        <Icon
                          className="settings-nav-icon"
                          size="1em"
                          aria-hidden="true"
                        />
                        <span className="settings-nav-label">
                          {t(`settings.group.${id}`)}
                        </span>
                        {badge(id)}
                      </button>
                      {current === id && entries.length > 1 && (
                        <ul
                          className="settings-subnav"
                          aria-label={t("settings.subsections")}
                        >
                          {entries.map((entry) => (
                            <li key={entry.id}>
                              <button
                                type="button"
                                className={
                                  selected === entry.id
                                    ? "sb-active"
                                    : undefined
                                }
                                aria-current={
                                  selected === entry.id ? "page" : undefined
                                }
                                onClick={() => {
                                  setGroup(entry.id);
                                }}
                              >
                                {t(entry.labelKey)}
                                {badge(entry.id)}
                              </button>
                            </li>
                          ))}
                        </ul>
                      )}
                      {id === "plugins" && current === id && (
                        <div className="settings-plugin-nav">
                          <h3>{t("settings.plugins.installed")}</h3>
                          <ul>
                            {management.installed?.map((plugin) => {
                              const target = `plugins/detail/${plugin.id}`;
                              const off = deactivated.includes(plugin.id);
                              return (
                                <li key={plugin.id}>
                                  <button
                                    type="button"
                                    className={
                                      selected === target
                                        ? "sb-active"
                                        : undefined
                                    }
                                    data-deactivated={off || undefined}
                                    aria-description={
                                      off
                                        ? t(
                                            "settings.plugins.status.deactivated",
                                          )
                                        : undefined
                                    }
                                    aria-current={
                                      selected === target ? "page" : undefined
                                    }
                                    onClick={() => {
                                      setGroup(target);
                                    }}
                                  >
                                    <span>{t(plugin.name ?? plugin.id)}</span>
                                    {badge(target)}
                                  </button>
                                </li>
                              );
                            })}
                          </ul>
                        </div>
                      )}
                    </div>
                  );
                })
              )}
            </nav>
            {!gated && <SettingsFooter />}
          </div>
          {!gated && (
            <label className="settings-compact-nav">
              <span className="sb-sr-only">{t("settings.groups")}</span>
              <select
                className="sb-select sb-select-native"
                value={selected}
                onChange={(event) => {
                  setGroup(event.target.value);
                }}
              >
                {SETTINGS_GROUPS.map((id) => (
                  <optgroup key={id} label={t(`settings.group.${id}`)}>
                    {pagesOf(id).map((entry) => (
                      <option key={entry.id} value={entry.id}>
                        {t(entry.labelKey)}
                      </option>
                    ))}
                    {id === "plugins" &&
                      management.installed?.map((plugin) => (
                        <option
                          key={plugin.id}
                          value={`plugins/detail/${plugin.id}`}
                          data-deactivated={
                            deactivated.includes(plugin.id) || undefined
                          }
                        >
                          {t(plugin.name ?? plugin.id)}
                        </option>
                      ))}
                  </optgroup>
                ))}
              </select>
            </label>
          )}
          <div
            key={gated ? "legal" : selected}
            id="settings-content"
            className="surface-scroll sb-scroll settings-group-content"
            role="region"
            aria-label={
              gated
                ? t("settings.group.legal")
                : t(page?.labelKey ?? "settings.plugins.pluginSettings")
            }
            tabIndex={0}
          >
            {gated ? (
              <LegalTab />
            ) : page?.kind === undefined ? (
              <SettingsBody page={selected} management={management} />
            ) : (
              <StorePage
                kind={page.kind}
                onBack={() => {
                  setGroup(page.kind === "theme" ? "bar/themes" : "plugins");
                }}
              />
            )}
          </div>
        </div>
      </div>
      {native && <ResizeEdges />}
    </div>
  );
}
