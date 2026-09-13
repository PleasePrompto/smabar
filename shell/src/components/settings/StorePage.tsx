import { ArrowLeft, RefreshCw } from "lucide-react";
import { useRef, useState } from "react";

import { safeIntlLocale, t } from "../../i18n/t";
import { call } from "../../ipc/call";
import {
  installStorePlugin,
  installStoreTheme,
  type StoreEntry,
  type StoreKind,
  type StoreOverview,
} from "../../ipc/store";
import { useSmabar } from "../../store/bar";
import { StoreDetail } from "./StoreDetail";
import { StoreList } from "./StoreList";
import {
  DEFAULT_STORE_SORT,
  filterEntries,
  formatDate,
  sortEntries,
  type StoreSortKey,
} from "./storeModel";
import { useStoreAction, type StoreEntryActions } from "./useStoreAction";
import { useStoreOverview } from "./useStoreOverview";

/** The orders the list offers; each key sorts the one way that is useful. */
const SORT_OPTIONS: readonly { key: StoreSortKey; labelKey: string }[] = [
  { key: "state", labelKey: "settings.store.sortRecommended" },
  { key: "name", labelKey: "settings.store.sortName" },
  { key: "stars", labelKey: "settings.store.sortStars" },
  { key: "updatedAt", labelKey: "settings.store.sortNewest" },
];

/** Where the catalog stands: its date, the last fetch, or why there is none. */
function CatalogStatus({
  overview,
  language,
}: {
  overview: StoreOverview;
  language: string;
}) {
  const checked =
    overview.fetchedAt === null
      ? null
      : new Intl.DateTimeFormat(safeIntlLocale(language), {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(overview.fetchedAt);
  if (overview.catalogState === "unavailable") {
    return (
      <div className="sb-alert sb-alert--warn" role="status">
        <div className="sb-alert__text">
          {t("settings.store.catalogUnavailable")}
          {overview.lastError !== null && (
            <>
              {" "}
              {t("settings.store.catalogError").replace(
                "{error}",
                overview.lastError,
              )}
            </>
          )}
        </div>
      </div>
    );
  }
  return (
    <p className="sb-faint sb-text-xs">
      {overview.catalogState === "stale"
        ? t("settings.store.catalogStale").replace("{checked}", checked ?? "")
        : t("settings.store.catalogFresh")
            .replace(
              "{generated}",
              overview.generatedAt === null
                ? ""
                : formatDate(overview.generatedAt, language),
            )
            .replace("{checked}", checked ?? "")}
    </p>
  );
}

/**
 * One Community Store page: the plugins or the themes of the catalog as a
 * searchable list in a chosen order with the buttons on every row, and the
 * detail page of the listing whose Details was pressed. `onBack` returns to the
 * settings group the page is listed under.
 */
export function StorePage({
  kind,
  onBack,
  initialEntryId,
}: {
  kind: StoreKind;
  onBack?: () => void;
  initialEntryId?: string;
}) {
  const language = useSmabar((state) => state.language);
  const { overview, refreshing, error, refresh, reload, apply } =
    useStoreOverview();
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<StoreSortKey>(DEFAULT_STORE_SORT);
  const [selectedId, setSelectedId] = useState<string | null>(
    initialEntryId ?? null,
  );
  const page = useRef<HTMLElement>(null);

  const entries =
    overview?.entries.filter((entry) => entry.kind === kind) ?? [];
  const shown = sortEntries(filterEntries(entries, query), sort);

  const actions: StoreEntryActions = {
    install: async (entry, { confirmModified }) => {
      const next =
        entry.kind === "plugin"
          ? await installStorePlugin(entry.id, entry.version, {
              confirmModified,
            })
          : await installStoreTheme(entry.id, entry.version);
      apply(next);
    },
    uninstall: async (entry: StoreEntry) => {
      if (entry.kind === "plugin") {
        await call("remove_plugin", { pluginId: entry.id });
      } else {
        await call("delete_theme", { name: entry.id });
      }
      reload();
    },
  };
  const action = useStoreAction(actions);

  const selected =
    selectedId === null
      ? null
      : (entries.find((entry) => entry.id === selectedId) ?? null);
  if (selected !== null && overview !== null) {
    return (
      <StoreDetail
        entry={selected}
        overview={overview}
        actions={actions}
        onBack={() => {
          setSelectedId(null);
          // The row's Details button exists again after the re-render.
          requestAnimationFrame(() => {
            page.current
              ?.querySelector<HTMLElement>(
                `[data-store-entry="${selected.kind}:${selected.id}"] [data-store-details]`,
              )
              ?.focus();
          });
        }}
      />
    );
  }

  const plugins = kind === "plugin";
  const title = t(
    plugins ? "settings.store.title" : "settings.themes.communityTitle",
  );
  return (
    <section
      ref={page}
      className="settings-group settings-store-page"
      aria-label={title}
    >
      <div className="settings-group-header">
        <div className="settings-store-page-title">
          {onBack !== undefined && (
            <button
              type="button"
              className="sb-btn sb-btn-ghost sb-btn-icon"
              aria-label={t("settings.store.back")}
              title={t("settings.store.back")}
              onClick={onBack}
            >
              <ArrowLeft size="1em" />
            </button>
          )}
          <h2>{title}</h2>
        </div>
        <button
          type="button"
          className="sb-btn sb-btn-ghost"
          disabled={refreshing}
          onClick={refresh}
        >
          <RefreshCw size="1em" aria-hidden="true" />
          {t(
            refreshing ? "settings.store.refreshing" : "settings.store.refresh",
          )}
        </button>
      </div>
      <p className="settings-help">
        {t(
          plugins
            ? "settings.store.description"
            : "settings.themes.communityDescription",
        )}
      </p>
      {error !== null && (
        <p className="sb-crit" role="alert">
          {error}
        </p>
      )}
      {overview !== null && (
        <CatalogStatus overview={overview} language={language} />
      )}
      <div className="settings-store-toolbar">
        <input
          type="search"
          className="sb-input"
          value={query}
          placeholder={t("settings.store.searchPlaceholder")}
          aria-label={t("settings.store.search")}
          onChange={(event) => {
            setQuery(event.target.value);
          }}
        />
        <select
          className="sb-select sb-select-native settings-store-sort"
          aria-label={t("settings.store.sortBy")}
          value={sort}
          onChange={(event) => {
            const chosen = event.currentTarget.value;
            const option = SORT_OPTIONS.find((entry) => entry.key === chosen);
            if (option !== undefined) setSort(option.key);
          }}
        >
          {SORT_OPTIONS.map((option) => (
            <option key={option.key} value={option.key}>
              {t(option.labelKey)}
            </option>
          ))}
        </select>
        <p className="sb-faint sb-text-xs" role="status" aria-live="polite">
          {overview === null
            ? t("settings.store.loading")
            : t("settings.store.count")
                .replace("{shown}", String(shown.length))
                .replace("{total}", String(entries.length))}
        </p>
      </div>
      {action.error !== null && (
        <p className="sb-crit" role="alert">
          {action.error}
        </p>
      )}
      {overview !== null && shown.length > 0 && (
        <StoreList
          entries={shown}
          overview={overview}
          language={language}
          onDetails={(entry) => {
            setSelectedId(entry.id);
          }}
          action={action}
        />
      )}
      {overview !== null && shown.length === 0 && (
        <p className="sb-faint">
          {t(
            entries.length === 0
              ? "settings.store.empty"
              : "settings.store.noMatch",
          )}
        </p>
      )}
    </section>
  );
}
