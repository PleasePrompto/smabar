import { useState } from "react";

import { reportError, visibleError } from "../../ipc/log";
import type { StoreEntry } from "../../ipc/store";
import type { StoreAction } from "./storeModel";
import { stateOf } from "./storeModel";

/** What the page does when the user confirms; both resolve on success. */
export interface StoreEntryActions {
  install: (
    entry: StoreEntry,
    options: { confirmModified: boolean },
  ) => Promise<void>;
  uninstall: (entry: StoreEntry) => Promise<void>;
}

export interface PendingConfirm {
  id: string;
  action: StoreAction;
}

export interface StoreActionState {
  /** The question currently asked, if any. */
  confirming: PendingConfirm | null;
  /** An action is running; every button waits. */
  busy: boolean;
  /** Why the last action failed; cleared by the next one. */
  error: string | null;
  ask: (entry: StoreEntry, action: StoreAction) => void;
  cancel: () => void;
  /** Runs the confirmed action; `returnFocus` fires once it settled. */
  confirm: (entry: StoreEntry, returnFocus: () => void) => void;
}

/**
 * The ask-then-run protocol every store button follows: nothing installs or
 * uninstalls on the first click, the question names the entry, and a
 * failure stays visible next to the button that caused it.
 */
export function useStoreAction(actions: StoreEntryActions): StoreActionState {
  const [confirming, setConfirming] = useState<PendingConfirm | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = (entry: StoreEntry, returnFocus: () => void) => {
    const action = confirming?.action;
    if (action === undefined) return;
    setConfirming(null);
    setBusy(true);
    setError(null);
    const task =
      action === "uninstall"
        ? actions.uninstall(entry)
        : actions.install(entry, {
            confirmModified: stateOf(entry) === "modified",
          });
    task
      .catch((cause: unknown) => {
        setError(visibleError(cause));
        reportError(cause);
      })
      .finally(() => {
        setBusy(false);
        requestAnimationFrame(returnFocus);
      });
  };

  return {
    confirming,
    busy,
    error,
    ask: (entry, action) => {
      setConfirming({ id: entry.id, action });
    },
    cancel: () => {
      setConfirming(null);
    },
    confirm,
  };
}
