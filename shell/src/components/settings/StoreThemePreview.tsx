import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError, visibleError } from "../../ipc/log";
import type { StoreEntry } from "../../ipc/store";
import { useSmabar, type ThemeSummary } from "../../store/bar";
import { ThemePreview, ThemeSwatches } from "./ThemePreview";

/** Only visible cards fetch their small, hash-verified theme file. */
export function StoreThemePreview({
  entry,
  onDetails,
  children,
}: {
  entry: StoreEntry;
  onDetails?: () => void;
  children?: ReactNode;
}) {
  const host = useRef<HTMLDivElement>(null);
  const descriptionId = useId();
  const layout = useSmabar((state) => state.layout);
  const [retry, setRetry] = useState(0);
  const [result, setResult] = useState<{
    key: string;
    preview: ThemeSummary["preview"] | null;
    error: string | null;
  } | null>(null);
  const key = `${entry.id}:${entry.commit}:${String(retry)}`;
  const current = result?.key === key ? result : null;
  useEffect(() => {
    let disposed = false;
    const load = () => {
      call<ThemeSummary["preview"]>("store_theme_preview", {
        name: entry.id,
        expectedCommit: entry.commit,
      })
        .then((preview) => {
          if (!disposed) setResult({ key, preview, error: null });
        })
        .catch((cause: unknown) => {
          if (!disposed)
            setResult({ key, preview: null, error: visibleError(cause) });
          reportError(cause);
        });
    };
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((item) => item.isIntersecting)) {
          observer.disconnect();
          load();
        }
      },
      { rootMargin: "160px" },
    );
    if (host.current) observer.observe(host.current);
    return () => {
      disposed = true;
      observer.disconnect();
    };
  }, [entry.id, entry.commit, key, layout]);
  const preview = current?.preview;
  return (
    <div ref={host} className="settings-store-theme-preview">
      {preview ? (
        onDetails ? (
          <button
            type="button"
            className="settings-theme-preview-trigger"
            aria-label={entry.name}
            aria-describedby={descriptionId}
            onClick={onDetails}
          >
            <ThemePreview preview={preview} descriptionId={descriptionId} />
          </button>
        ) : (
          <figure
            className="settings-theme-preview-trigger"
            tabIndex={0}
            aria-label={entry.name}
            aria-describedby={descriptionId}
          >
            <ThemePreview preview={preview} descriptionId={descriptionId} />
          </figure>
        )
      ) : (
        <div className="settings-theme-preview-placeholder" role="status">
          <span>
            {t(
              current?.error
                ? "settings.themes.preview.failed"
                : "settings.themes.preview.loading",
            )}
          </span>
          {current?.error && (
            <>
              <small>{current.error}</small>
              <button
                type="button"
                className="sb-btn sb-btn-ghost"
                onClick={() => {
                  setRetry((value) => value + 1);
                }}
              >
                {t("settings.themes.preview.retry")}
              </button>
            </>
          )}
        </div>
      )}
      <div className="settings-theme-actions">
        {preview && <ThemeSwatches preview={preview} />}
        {children}
      </div>
    </div>
  );
}
