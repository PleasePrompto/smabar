import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { call } from "../../ipc/call";
import { cleanupListeners } from "../../ipc/listeners";
import { reportError } from "../../ipc/log";
import { setConfigsSequentially } from "./persist";

export interface AudioLevel {
  volume: number;
  muted: boolean;
}

export interface AudioConfig extends AudioLevel {
  notificationSounds: boolean;
  plugins: Record<string, AudioLevel>;
}

/** Mounted once per settings page, regardless of how many cards it displays. */
export function useAudioSettings() {
  const [config, setConfig] = useState<AudioConfig | null>(null);
  const [failedPath, setFailedPath] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const mounted = useRef(false);
  const revision = useRef(0);
  const pending = useRef(false);

  const refresh = useCallback(() => {
    const current = ++revision.current;
    return call<AudioConfig>("get_audio_settings")
      .then((next) => {
        if (mounted.current && current === revision.current) {
          setConfig(next);
          setFailedPath(null);
        }
      })
      .catch((cause: unknown) => {
        reportError(cause);
        if (mounted.current && current === revision.current)
          setFailedPath("audio");
      });
  }, []);

  useEffect(() => {
    mounted.current = true;
    const unlisten = cleanupListeners(
      "__TAURI_INTERNALS__" in window
        ? [
            listen<AudioConfig>("audio-settings-changed", ({ payload }) => {
              if (!mounted.current) return;
              ++revision.current;
              setConfig(payload);
            }),
          ]
        : [],
    );
    void refresh();
    return () => {
      mounted.current = false;
      unlisten();
    };
  }, [refresh]);

  const write = async (path: string, value: number | boolean) => {
    if (pending.current) return;
    pending.current = true;
    setSaving(true);
    setFailedPath(null);
    try {
      await setConfigsSequentially([{ path, value }]);
      await refresh();
    } catch (cause: unknown) {
      reportError(cause);
      if (mounted.current) setFailedPath(path);
    } finally {
      pending.current = false;
      if (mounted.current) setSaving(false);
    }
  };

  return { config, failedPath, saving, write, refresh };
}

export type AudioSettings = ReturnType<typeof useAudioSettings>;
