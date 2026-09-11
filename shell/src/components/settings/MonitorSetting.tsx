import { useEffect, useState } from "react";

import { t } from "../../i18n/t";
import { cleanupListeners } from "../../ipc/listeners";
import { reportError } from "../../ipc/log";
import {
  getMonitorState,
  onMonitorsChanged,
  type MonitorState,
} from "../../ipc/monitors";
import { useSmabar, type MonitorPreference } from "../../store/bar";
import { setConfig } from "./persist";

export function MonitorSetting() {
  const selected = useSmabar((state) => state.layout.monitor);
  const [state, setState] = useState<MonitorState | null>(null);

  useEffect(() => {
    let disposed = false;
    let generation = 0;
    const refresh = () => {
      const current = ++generation;
      void getMonitorState()
        .then((next) => {
          if (!disposed && current === generation) setState(next);
        })
        .catch(reportError);
    };
    refresh();
    const cleanup = cleanupListeners([onMonitorsChanged(refresh)]);
    return () => {
      disposed = true;
      cleanup();
    };
  }, []);

  const monitors = state?.monitors ?? [];
  const disconnected =
    selected !== null && state !== null && !state.preferredConnected;
  const choose = (preference: MonitorPreference | null) => {
    const store = useSmabar.getState();
    store.setLayout({ ...store.layout, monitor: preference });
    setConfig("layout.monitor", preference);
  };

  return (
    <div className="settings-monitor-control">
      <select
        className="sb-select sb-select-native"
        aria-label={t("settings.monitor.label")}
        value={selected?.id ?? ""}
        onChange={(event) => {
          const id = event.currentTarget.value;
          const monitor = monitors.find((candidate) => candidate.id === id);
          choose(monitor === undefined ? null : { id, label: monitor.label });
        }}
      >
        <option value="">{t("settings.monitor.automatic")}</option>
        {monitors.map((monitor) => (
          <option key={monitor.id} value={monitor.id}>
            {`${monitor.label} — ${String(monitor.width)}×${String(monitor.height)}${monitor.primary ? ` (${t("settings.monitor.primary")})` : ""}`}
          </option>
        ))}
        {disconnected && (
          <option value={selected.id}>
            {`${selected.label} (${t("settings.monitor.disconnected")})`}
          </option>
        )}
      </select>
      {disconnected && (
        <span className="sb-warn" role="status">
          {t("settings.monitor.fallback")}
        </span>
      )}
    </div>
  );
}
