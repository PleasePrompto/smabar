import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { call } from "../../ipc/call";
import { cleanupListeners } from "../../ipc/listeners";
import { reportError } from "../../ipc/log";

export type AutostartStatus =
  | { state: "unavailable" }
  | { state: "ready"; registered: boolean }
  | { state: "failed"; registered: boolean | null };

/** The OS snapshot is shared with the tray; writes never optimistically flip it. */
export function useAutostart() {
  const [status, setStatus] = useState<AutostartStatus | null>(null);
  const [busy, setBusy] = useState(true);
  const [failed, setFailed] = useState(false);
  const mounted = useRef(false);
  const revision = useRef(0);
  const pending = useRef<Promise<void> | null>(null);

  const execute = useCallback((enabled?: boolean) => {
    const version = ++revision.current;
    setBusy(true);
    setFailed(false);
    const task = (async () => {
      try {
        const next = await (enabled === undefined
          ? call<AutostartStatus>("get_autostart_status")
          : call<AutostartStatus>("set_autostart", { enabled }));
        if (mounted.current && version === revision.current) setStatus(next);
      } catch (cause: unknown) {
        reportError(cause);
        if (mounted.current && version === revision.current) setFailed(true);
      } finally {
        if (mounted.current) setBusy(false);
        pending.current = null;
      }
    })();
    pending.current = task;
    return task;
  }, []);

  const refresh = useCallback(() => pending.current ?? execute(), [execute]);
  const setEnabled = useCallback(
    async (enabled: boolean) => {
      await pending.current;
      await execute(enabled);
    },
    [execute],
  );

  useEffect(() => {
    mounted.current = true;
    const onFocus = () => {
      void refresh();
    };
    const unlisten = cleanupListeners(
      "__TAURI_INTERNALS__" in window
        ? [
            listen<AutostartStatus>("autostart-changed", ({ payload }) => {
              if (!mounted.current) return;
              ++revision.current;
              setStatus(payload);
              setFailed(false);
            }),
          ]
        : [],
    );
    void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      mounted.current = false;
      window.removeEventListener("focus", onFocus);
      unlisten();
    };
  }, [refresh]);

  return {
    status,
    busy,
    failed: failed || status?.state === "failed",
    refresh,
    setEnabled,
  };
}
