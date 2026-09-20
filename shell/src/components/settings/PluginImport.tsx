import { FileUp } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError, visibleError } from "../../ipc/log";
import { cleanupListeners } from "../../ipc/listeners";
import { onStoreProgress, type StoreProgress } from "../../ipc/store";
import { ConfirmRow } from "./controls";
import { Progress } from "./storeActions";
import { useSmabar } from "../../store/bar";

interface Preview {
  id: string;
  name: string;
  version: string;
  previousVersion: string | null;
  previousDigest: string | null;
  archiveSha256: string;
  community: boolean;
}
export function PluginImport({
  onInstalled,
}: {
  onInstalled: () => Promise<void>;
}) {
  const [preview, setPreview] = useState<{
    path: string;
    plugin: Preview;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const lock = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const [progress, setProgress] = useState<StoreProgress | null>(null);
  const language = useSmabar((state) => state.language);
  const button = useRef<HTMLButtonElement>(null);
  useEffect(
    () =>
      cleanupListeners([
        onStoreProgress((event) => {
          setProgress(event);
        }),
      ]),
    [],
  );
  async function run(operation: () => Promise<void>) {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError(null);
    setDone(false);
    setProgress(null);
    try {
      await operation();
    } catch (cause: unknown) {
      reportError(cause);
      setError(visibleError(cause));
    } finally {
      lock.current = false;
      setBusy(false);
      requestAnimationFrame(() => button.current?.focus());
    }
  }
  return (
    <div className="settings-import-panel">
      <div>
        <strong>{t("settings.plugins.zipTitle")}</strong>
        <p className="settings-help">{t("settings.plugins.zipDescription")}</p>
      </div>
      <button
        ref={button}
        type="button"
        className="sb-btn sb-btn-primary"
        disabled={busy}
        onClick={() => {
          void run(async () => {
            const path = await call<string | null>("choose_settings_file", {
              purpose: "pluginZip",
            });
            if (path === null) return;
            setPreview(null);
            const plugin = await call<Preview>("inspect_plugin_zip", { path });
            setPreview({ path, plugin });
          });
        }}
      >
        <FileUp size="1em" />
        {t(busy ? "settings.plugins.zipWorking" : "settings.plugins.zipImport")}
      </button>
      {preview !== null && !busy && (
        <ConfirmRow
          label={preview.plugin.name}
          question={t(
            preview.plugin.previousDigest === null
              ? "settings.plugins.zipConfirm"
              : preview.plugin.community
                ? "settings.plugins.zipReplaceCommunity"
                : "settings.plugins.zipReplace",
          )
            .replace("{name}", preview.plugin.name)
            .replace("{version}", preview.plugin.version)
            .replace("{previous}", preview.plugin.previousVersion ?? "?")}
          action={t(
            preview.plugin.previousDigest === null
              ? "settings.plugins.zipInstall"
              : "settings.plugins.zipReplaceAction",
          )}
          onCancel={() => {
            setPreview(null);
          }}
          returnFocus={() => button.current?.focus()}
          onConfirm={() => {
            void run(async () => {
              await call("install_plugin_zip", {
                path: preview.path,
                archiveSha256: preview.plugin.archiveSha256,
                previousDigest: preview.plugin.previousDigest,
              });
              setPreview(null);
              setDone(true);
              await onInstalled();
            });
          }}
        />
      )}
      {busy && progress !== null && progress.id === preview?.plugin.id && (
        <Progress progress={progress} language={language} />
      )}
      {error !== null && (
        <p className="sb-crit" role="alert">
          {error}
        </p>
      )}
      {done && (
        <p role="status" className="settings-help">
          {t("settings.plugins.zipDone")}
        </p>
      )}
    </div>
  );
}
