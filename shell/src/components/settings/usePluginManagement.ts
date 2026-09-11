import { useCallback, useEffect, useRef, useState } from "react";

import { call } from "../../ipc/call";
import { cleanupListeners } from "../../ipc/listeners";
import { reportError } from "../../ipc/log";
import { onStoreChanged } from "../../ipc/store";
import { useSmabar, type InstalledPlugin } from "../../store/bar";
import { toggleDisabled } from "./model";
import { setConfigsSequentially } from "./persist";

/** One installed snapshot and mutation path for the sort list and cards. */
export function usePluginManagement() {
  const registryVersion = useSmabar((state) => state.registryVersion);
  const deactivated = useSmabar((state) => state.pluginsDeactivated);
  const statuses = useSmabar((state) => state.pluginStatus);
  const [installed, setInstalled] = useState<InstalledPlugin[] | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [failedPlugin, setFailedPlugin] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const mounted = useRef(false);
  const revision = useRef(0);

  const refresh = useCallback(() => {
    const current = ++revision.current;
    return call<InstalledPlugin[]>("list_plugins")
      .then((plugins) => {
        if (mounted.current && current === revision.current) {
          setInstalled(plugins);
          setLoadFailed(false);
        }
      })
      .catch((cause: unknown) => {
        reportError(cause);
        if (mounted.current && current === revision.current)
          setLoadFailed(true);
      });
  }, []);

  useEffect(() => {
    mounted.current = true;
    const unlisten = cleanupListeners([onStoreChanged(() => void refresh())]);
    return () => {
      mounted.current = false;
      unlisten();
    };
  }, [refresh]);

  useEffect(() => {
    void refresh();
  }, [refresh, registryVersion, deactivated, statuses]);

  const perform = async (
    pluginId: string,
    operation: () => Promise<unknown>,
  ) => {
    // Both visibility and activation write shared arrays; serialize gestures
    // across the two views so a second click cannot replace the first write.
    if (pending.current) return false;
    pending.current = true;
    setBusy(true);
    setFailedPlugin(null);
    try {
      await operation();
      await refresh();
      return true;
    } catch (cause: unknown) {
      reportError(cause);
      if (mounted.current) setFailedPlugin(pluginId);
      return false;
    } finally {
      pending.current = false;
      if (mounted.current) setBusy(false);
    }
  };

  return {
    installed,
    loadFailed,
    failedPlugin,
    busy,
    refresh,
    toggleHidden: (pluginId: string, tileId: string) =>
      perform(pluginId, async () => {
        const next = toggleDisabled(useSmabar.getState().pluginsHidden, tileId);
        await setConfigsSequentially([{ path: "pluginsHidden", value: next }]);
        useSmabar.getState().setPluginsHidden(next);
      }),
    toggleActive: (pluginId: string) =>
      perform(pluginId, async () => {
        const next = toggleDisabled(
          useSmabar.getState().pluginsDeactivated,
          pluginId,
        );
        await setConfigsSequentially([
          { path: "pluginsDeactivated", value: next },
        ]);
        useSmabar.getState().setPluginsDeactivated(next);
      }),
    remove: (pluginId: string) =>
      perform(pluginId, async () => {
        await call("remove_plugin", { pluginId });
        if (mounted.current)
          setInstalled(
            (current) =>
              current?.filter((plugin) => plugin.id !== pluginId) ?? null,
          );
      }),
  };
}

export type PluginManagement = ReturnType<typeof usePluginManagement>;
