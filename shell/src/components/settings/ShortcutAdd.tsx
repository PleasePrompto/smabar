import { Pin, Plus } from "lucide-react";
import { useEffect, useState } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import type { SpecialShortcut } from "../../store/bar";
import { IconOrInitial, SettingGroup, SettingRow } from "./controls";

type AppSource = { desktopId: string } | { path: string };

/** One `list_apps` result: an XDG id on Linux or a launch path on Windows/macOS. */
type AppEntry = {
  name: string;
  comment?: string;
} & ({ desktopId: string; path?: never } | { desktopId?: never; path: string });

const SPECIAL_ITEMS = [
  { source: "computer", label: "settings.shortcuts.specialComputer" },
  { source: "trash", label: "settings.shortcuts.specialTrash" },
] as const satisfies readonly {
  source: SpecialShortcut;
  label: string;
}[];

const SEARCH_DEBOUNCE_MS = 200;
/** Rendered result cap — icons load per rendered row, so this bounds work. */
const MAX_RESULTS = 20;
type SearchState = "pending" | "ready" | "failed";

/** In-memory icon cache (data URI or null = unresolved), keyed by app source. */
const iconCache = new Map<string, string | null>();

function appSource(app: AppEntry): AppSource {
  return app.desktopId === undefined
    ? { path: app.path }
    : { desktopId: app.desktopId };
}

function appKey(app: AppEntry): string {
  return app.desktopId === undefined
    ? `path:${app.path}`
    : `desktop:${app.desktopId}`;
}

function AppIcon({ app }: { app: AppEntry }) {
  const key = appKey(app);
  const [, setLoaded] = useState(0);
  useEffect(() => {
    if (iconCache.has(key)) return;
    let cancelled = false;
    call<string | null>("get_app_icon", appSource(app))
      .then((uri) => {
        iconCache.set(key, uri);
        if (!cancelled) setLoaded((count) => count + 1);
      })
      .catch(reportError);
    return () => {
      cancelled = true;
    };
  }, [app, key]);
  return <IconOrInitial icon={iconCache.get(key)} label={app.name} />;
}

/** Application search plus the non-search shortcut sources. */
export function ShortcutAdd() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<AppEntry[]>([]);
  const [searchState, setSearchState] = useState<SearchState>("pending");
  const [website, setWebsite] = useState("");
  const [websiteFailed, setWebsiteFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const handle = window.setTimeout(() => {
      call<AppEntry[]>("list_apps", query === "" ? undefined : { query })
        .then((apps) => {
          if (cancelled) return;
          setResults(apps.slice(0, MAX_RESULTS));
          setSearchState("ready");
        })
        .catch((error: unknown) => {
          if (cancelled) return;
          setResults([]);
          setSearchState("failed");
          reportError(error);
        });
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [query]);

  const addWebsite = () => {
    const value = website.trim();
    if (value === "") return;
    // A bare host is what people type into a taskbar; everything else is
    // validated (and rejected) by the core.
    const url = /^https?:\/\//i.test(value) ? value : `https://${value}`;
    call("pin_shortcut", { url })
      .then(() => {
        setWebsite("");
        setWebsiteFailed(false);
      })
      .catch((error: unknown) => {
        setWebsiteFailed(true);
        reportError(error);
      });
  };

  return (
    <SettingGroup title={t("settings.shortcuts.add")}>
      <SettingRow
        label={t("settings.shortcuts.search")}
        description={t("settings.shortcuts.searchDescription")}
        wide
      >
        <input
          className="sb-input"
          type="search"
          value={query}
          aria-label={t("settings.shortcuts.search")}
          autoComplete="off"
          placeholder={t("settings.shortcuts.search")}
          onChange={(event) => {
            setQuery(event.target.value);
            setSearchState("pending");
          }}
        />
        <div
          className="sb-list sb-scroll"
          style={{ maxHeight: "11rem", marginTop: "var(--sb-space-xs)" }}
          aria-live="polite"
          aria-busy={searchState === "pending"}
        >
          {searchState === "pending" && (
            <div className="sb-faint">{t("settings.shortcuts.searching")}</div>
          )}
          {searchState === "failed" && (
            <div className="sb-error" role="alert">
              {t("settings.shortcuts.searchFailed")}
            </div>
          )}
          {searchState === "ready" &&
            results.map((app) => (
              <div key={appKey(app)} className="sb-row">
                <AppIcon app={app} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="settings-ellipsis">{app.name}</div>
                  {app.comment !== undefined && (
                    <div className="sb-faint sb-text-xs settings-ellipsis">
                      {app.comment}
                    </div>
                  )}
                </div>
                <button
                  type="button"
                  className="sb-btn sb-btn-ghost sb-btn-icon"
                  aria-label={t("settings.shortcuts.pin")}
                  title={t("settings.shortcuts.pin")}
                  onClick={() => {
                    void call("pin_shortcut", appSource(app)).catch(
                      reportError,
                    );
                  }}
                >
                  <Pin size="1em" />
                </button>
              </div>
            ))}
          {searchState === "ready" && results.length === 0 && (
            <div className="sb-faint">{t("settings.shortcuts.noResults")}</div>
          )}
        </div>
      </SettingRow>

      <SettingRow
        label={t("settings.shortcuts.special")}
        description={t("settings.shortcuts.specialDescription")}
      >
        <div className="flex flex-wrap gap-2">
          {SPECIAL_ITEMS.map((item) => {
            const label = t(item.label);
            return (
              <button
                key={item.source}
                type="button"
                className="sb-btn sb-btn-ghost"
                aria-label={label}
                onClick={() => {
                  void call("pin_special_shortcut", {
                    special: item.source,
                  }).catch(reportError);
                }}
              >
                <Plus size="1em" />
                {label}
              </button>
            );
          })}
        </div>
      </SettingRow>

      <SettingRow
        label={t("settings.shortcuts.website")}
        description={t("settings.shortcuts.websiteDescription")}
        wide
      >
        <form
          className="sb-field"
          onSubmit={(event) => {
            event.preventDefault();
            addWebsite();
          }}
        >
          <input
            className="sb-input"
            type="text"
            name="website"
            inputMode="url"
            value={website}
            aria-label={t("settings.shortcuts.website")}
            aria-invalid={websiteFailed}
            aria-describedby={
              websiteFailed ? "settings-website-error" : undefined
            }
            autoComplete="off"
            placeholder={t("settings.shortcuts.websitePlaceholder")}
            onChange={(event) => {
              setWebsite(event.target.value);
              setWebsiteFailed(false);
            }}
          />
          <button className="sb-btn sb-btn-ghost" type="submit">
            <Plus size="1em" />
            {t("settings.shortcuts.addWebsite")}
          </button>
        </form>
        {websiteFailed && (
          <p id="settings-website-error" className="sb-error" role="alert">
            {t("settings.shortcuts.websiteFailed")}
          </p>
        )}
      </SettingRow>

      <SettingRow
        label={t("settings.shortcuts.separator")}
        description={t("settings.shortcuts.separatorDescription")}
      >
        <button
          type="button"
          className="sb-btn sb-btn-ghost"
          onClick={() => {
            void call("pin_shortcut", { separator: true }).catch(reportError);
          }}
        >
          <Plus size="1em" />
          {t("settings.shortcuts.addSeparator")}
        </button>
      </SettingRow>
    </SettingGroup>
  );
}
