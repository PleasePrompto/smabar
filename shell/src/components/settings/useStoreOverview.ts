import { useCallback, useEffect, useRef, useState } from "react";

import { cleanupListeners } from "../../ipc/listeners";
import { reportError, visibleError } from "../../ipc/log";
import {
  onStoreChanged,
  onStoreProgress,
  storeOverview,
  storeRefresh,
  type StoreOverview,
} from "../../ipc/store";

export interface StoreOverviewState {
  /** Null until the first read answered. */
  overview: StoreOverview | null;
  /** A network refresh is running. */
  refreshing: boolean;
  /** Why the last refresh or read failed; cleared by the next attempt. */
  error: string | null;
  /** Fetches the catalog again (network). */
  refresh: () => void;
  /** Re-reads the cached overview, e.g. after an uninstall. */
  reload: () => void;
  /** Adopts the overview a mutating command answered with. */
  apply: (overview: StoreOverview) => void;
}

/**
 * The store overview a settings group renders from. Read once from the
 * core's cache, refreshed once per mount (a conditional GET — opening the
 * group is the one moment a user expects a current catalog), re-read on
 * every `store-changed` (the core's own timer, another window's install),
 * and patched by `store-progress` while an install runs. Nothing here
 * polls: the refresh timer lives in the core.
 */
export function useStoreOverview(): StoreOverviewState {
  const [overview, setOverview] = useState<StoreOverview | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // A read that is still in flight when the group unmounts must not set state.
  const mounted = useRef(true);

  const reload = useCallback(() => {
    storeOverview()
      .then((next) => {
        if (!mounted.current) return;
        setOverview(next);
        setError(null);
      })
      .catch((cause: unknown) => {
        if (mounted.current) setError(visibleError(cause));
        reportError(cause);
      });
  }, []);

  useEffect(() => {
    mounted.current = true;
    reload();
    // One conditional GET per mount; state only moves once it answers.
    storeRefresh()
      .then((next) => {
        if (mounted.current) setOverview(next);
      })
      .catch((cause: unknown) => {
        if (mounted.current) setError(visibleError(cause));
        reportError(cause);
      });
    const stop = cleanupListeners([
      onStoreChanged(reload),
      onStoreProgress((progress) => {
        setOverview((current) =>
          current === null ? current : { ...current, pending: progress },
        );
      }),
    ]);
    return () => {
      mounted.current = false;
      stop();
    };
  }, [reload]);

  const refresh = useCallback(() => {
    setRefreshing(true);
    setError(null);
    storeRefresh()
      .then((next) => {
        if (mounted.current) setOverview(next);
      })
      .catch((cause: unknown) => {
        if (mounted.current) setError(visibleError(cause));
        reportError(cause);
      })
      .finally(() => {
        if (mounted.current) setRefreshing(false);
      });
  }, []);

  const apply = useCallback((next: StoreOverview) => {
    setOverview(next);
  }, []);

  return { overview, refreshing, error, refresh, reload, apply };
}
