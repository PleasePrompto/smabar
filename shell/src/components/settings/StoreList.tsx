import { Star } from "lucide-react";
import { useRef } from "react";

import { t } from "../../i18n/t";
import type { StoreEntry, StoreOverview } from "../../ipc/store";
import { ConfirmRow } from "./controls";
import { StoreStateBadge } from "./StoreBadge";
import {
  ActionButtons,
  actionLabel,
  confirmQuestion,
  Progress,
} from "./storeActions";
import {
  actionsFor,
  formatDate,
  formatStars,
  stateOf,
  type StoreAction,
} from "./storeModel";
import type { StoreActionState } from "./useStoreAction";

/** GitHub's star count: the one popularity signal the catalog carries. */
export function Stars({
  entry,
  language,
}: {
  entry: StoreEntry;
  language: string;
}) {
  return (
    <span
      className="settings-store-stars"
      title={t("settings.store.stars")}
      aria-label={t("settings.store.starsCount").replace(
        "{count}",
        String(entry.repo.stars),
      )}
    >
      <Star size="1em" aria-hidden="true" />
      {formatStars(entry.repo.stars, language)}
    </span>
  );
}

/** Author, version and listing date in one line under the name. */
export function Meta({
  entry,
  language,
}: {
  entry: StoreEntry;
  language: string;
}) {
  return (
    <p className="settings-store-meta">
      <span>{entry.author.login}</span>
      <span className="sb-mono">{entry.version}</span>
      <span>{formatDate(entry.updatedAt, language)}</span>
    </p>
  );
}

function Row({
  entry,
  overview,
  language,
  onDetails,
  action,
}: {
  entry: StoreEntry;
  overview: StoreOverview;
  language: string;
  onDetails: (entry: StoreEntry) => void;
  action: StoreActionState;
}) {
  const buttons = useRef(new Map<StoreAction, HTMLButtonElement>());
  const details = useRef<HTMLButtonElement>(null);
  const state = stateOf(entry);
  const offered = actionsFor(entry);
  const pending =
    overview.pending !== null &&
    overview.pending.kind === entry.kind &&
    overview.pending.id === entry.id
      ? overview.pending
      : null;
  const locked = action.busy || overview.pending !== null;
  const confirming =
    action.confirming !== null && action.confirming.id === entry.id
      ? action.confirming
      : null;
  // After an uninstall the button that asked is gone; Details always exists.
  const focusBack = (name: StoreAction) => {
    (buttons.current.get(name) ?? details.current)?.focus();
  };
  return (
    <li data-store-entry={`${entry.kind}:${entry.id}`}>
      <div className="settings-store-row">
        <div className="settings-store-main">
          <div className="settings-store-heading">
            <span className="settings-store-name">{entry.name}</span>
            <StoreStateBadge entry={entry} />
          </div>
          <Meta entry={entry} language={language} />
          <p className="settings-store-description">{entry.description}</p>
        </div>
        <div className="settings-store-side">
          <Stars entry={entry} language={language} />
          <button
            ref={details}
            type="button"
            className="sb-btn"
            data-store-details=""
            onClick={() => {
              onDetails(entry);
            }}
          >
            {t("settings.store.details")}
          </button>
          <ActionButtons
            entry={entry}
            offered={offered}
            disabled={locked}
            onAsk={(next) => {
              action.ask(entry, next);
            }}
            buttonRef={(name, node) => {
              if (node === null) buttons.current.delete(name);
              else buttons.current.set(name, node);
            }}
          />
        </div>
      </div>
      {(confirming !== null || pending !== null) && (
        <div className="settings-store-row-extra">
          {pending !== null && (
            <Progress progress={pending} language={language} />
          )}
          {confirming !== null && (
            <ConfirmRow
              label={entry.name}
              question={confirmQuestion(confirming.action, entry, state)}
              action={actionLabel(confirming.action, entry)}
              onCancel={action.cancel}
              onConfirm={() => {
                action.confirm(entry, () => {
                  focusBack(confirming.action);
                });
              }}
              returnFocus={() => {
                focusBack(confirming.action);
              }}
            />
          )}
        </div>
      )}
    </li>
  );
}

/**
 * The catalog as a list: one row per listing with the name, the facts that
 * decide whether to look closer, its stars, and the buttons on the row —
 * Details, then install, update or uninstall. Nothing installs on the first
 * click: the question opens under the row.
 */
export function StoreList({
  entries,
  overview,
  language,
  onDetails,
  action,
}: {
  entries: readonly StoreEntry[];
  overview: StoreOverview;
  language: string;
  onDetails: (entry: StoreEntry) => void;
  action: StoreActionState;
}) {
  return (
    <ul className="settings-store-list">
      {entries.map((entry) => (
        <Row
          key={entry.id}
          entry={entry}
          overview={overview}
          language={language}
          onDetails={onDetails}
          action={action}
        />
      ))}
    </ul>
  );
}
