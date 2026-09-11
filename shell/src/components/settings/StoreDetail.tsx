import { ArrowLeft } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { t } from "../../i18n/t";
import { reportError, visibleError } from "../../ipc/log";
import {
  storeDetail,
  type StoreDetailInfo,
  type StoreEntry,
  type StoreOverview,
} from "../../ipc/store";
import { useSmabar } from "../../store/bar";
import { ConfirmRow } from "./controls";
import { StoreStateBadge } from "./StoreBadge";
import { LinkButton, StoreFacts, StoreNotes, TrustFacts } from "./StoreFacts";
import { Meta, Stars } from "./StoreList";
import { StoreReadme } from "./StoreReadme";
import {
  ActionButtons,
  actionLabel,
  confirmQuestion,
  Progress,
} from "./storeActions";
import {
  actionsFor,
  formatDate,
  releaseUrl,
  stateOf,
  type StoreAction,
} from "./storeModel";
import { useStoreAction, type StoreEntryActions } from "./useStoreAction";

function Releases({
  detail,
  entry,
  language,
}: {
  detail: StoreDetailInfo;
  entry: StoreEntry;
  language: string;
}) {
  if (detail.releases.length === 0) {
    return <p className="sb-faint">{t("settings.store.releasesNone")}</p>;
  }
  return (
    <ul className="settings-store-releases">
      {detail.releases.map((release) => (
        <li key={release.tag}>
          <span className="sb-mono">{release.version}</span>
          {release.publishedAt !== null && (
            <span className="sb-faint">
              {formatDate(release.publishedAt, language)}
            </span>
          )}
          <LinkButton
            url={releaseUrl(entry.repo.url, release.tag)}
            label={release.tag}
          />
        </li>
      ))}
    </ul>
  );
}

/** The README as the core rendered it, or as text when it could not. */
function Readme({
  detail,
  detailError,
}: {
  detail: StoreDetailInfo | null;
  detailError: string | null;
}) {
  if (detailError !== null) {
    return (
      <p className="sb-crit" role="alert">
        {t("settings.store.detailFailed").replace("{error}", detailError)}
      </p>
    );
  }
  if (detail === null) {
    return (
      <p className="sb-faint" aria-live="polite">
        {t("settings.store.detailLoading")}
      </p>
    );
  }
  if (detail.readmeHtml !== null) {
    return (
      <div className="settings-store-readme">
        <StoreReadme html={detail.readmeHtml} />
      </div>
    );
  }
  if (detail.readme === null || detail.readme === "") {
    return <p className="sb-faint">{t("settings.store.readmeNone")}</p>;
  }
  return (
    <div className="settings-store-readme whitespace-pre-wrap">
      {detail.readme}
    </div>
  );
}

/**
 * The detail page of one listing: a header card with the name, the facts
 * from the list and the actions; then the notes a user must read before
 * installing; then the readme beside a sidebar of source facts. Every
 * action asks inline before doing anything; one install runs at a time
 * app-wide, so every button waits while the overview reports a pending one.
 */
export function StoreDetail({
  entry,
  overview,
  actions,
  onBack,
}: {
  entry: StoreEntry;
  overview: StoreOverview;
  actions: StoreEntryActions;
  onBack: () => void;
}) {
  const language = useSmabar((state) => state.language);
  const [detail, setDetail] = useState<StoreDetailInfo | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const buttons = useRef(new Map<StoreAction, HTMLButtonElement>());
  const backButton = useRef<HTMLButtonElement>(null);
  const action = useStoreAction(actions);

  // Loaded once: the page mounts a detail per listing and unmounts it on
  // the way back, so a mounted detail never changes its entry.
  useEffect(() => {
    let cancelled = false;
    storeDetail(entry.kind, entry.id)
      .then((info) => {
        if (!cancelled) setDetail(info);
      })
      .catch((cause: unknown) => {
        if (!cancelled) setDetailError(visibleError(cause));
        reportError(cause);
      });
    return () => {
      cancelled = true;
    };
  }, [entry.kind, entry.id]);

  // The list's Details button is gone once the page shows; the way back is
  // where the focus lands.
  useEffect(() => {
    backButton.current?.focus();
  }, []);

  const state = stateOf(entry);
  const offered = actionsFor(entry);
  const pending =
    overview.pending !== null &&
    overview.pending.kind === entry.kind &&
    overview.pending.id === entry.id
      ? overview.pending
      : null;
  const locked = action.busy || overview.pending !== null;
  const confirming = action.confirming;
  const focusAsker = () => {
    if (confirming !== null) buttons.current.get(confirming.action)?.focus();
  };

  return (
    <section
      className="settings-group settings-store-detail"
      role="region"
      aria-label={entry.name}
      data-store-entry={`${entry.kind}:${entry.id}`}
    >
      <button
        ref={backButton}
        type="button"
        className="sb-btn sb-btn-ghost settings-store-back"
        onClick={onBack}
      >
        <ArrowLeft size="1em" aria-hidden="true" />
        {t("settings.store.back")}
      </button>
      <header className="settings-store-hero">
        <div className="settings-store-main">
          <div className="settings-store-heading">
            <h2>{entry.name}</h2>
            <StoreStateBadge entry={entry} />
          </div>
          <Meta entry={entry} language={language} />
          <p className="settings-store-description">{entry.description}</p>
        </div>
        <div className="settings-store-side">
          <Stars entry={entry} language={language} />
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
        {action.error !== null && (
          <p className="sb-crit settings-store-hero-note" role="alert">
            {action.error}
          </p>
        )}
        {pending !== null && (
          <div className="settings-store-hero-note">
            <Progress progress={pending} language={language} />
          </div>
        )}
        {confirming !== null && (
          <div className="settings-store-hero-note">
            <ConfirmRow
              label={entry.name}
              question={confirmQuestion(confirming.action, entry, state)}
              action={actionLabel(confirming.action, entry)}
              onCancel={action.cancel}
              onConfirm={() => {
                action.confirm(entry, focusAsker);
              }}
              returnFocus={focusAsker}
            />
          </div>
        )}
      </header>

      <StoreNotes entry={entry} overview={overview} />

      <div className="settings-store-columns">
        <div className="settings-store-column">
          <div className="sb-section">{t("settings.store.readme")}</div>
          <Readme detail={detail} detailError={detailError} />
          {detail !== null && (
            <>
              <div className="sb-section">{t("settings.store.releases")}</div>
              <Releases detail={detail} entry={entry} language={language} />
            </>
          )}
        </div>
        <aside className="settings-store-aside">
          <TrustFacts entry={entry} overview={overview} />
          <StoreFacts entry={entry} language={language} />
          <LinkButton
            url={`https://github.com/PleasePrompto/smabar/issues/new?${new URLSearchParams(
              {
                title: `Report ${entry.kind}: ${entry.id}`,
                body: `${entry.repo.url}\n\n`,
              },
            )}`}
            label={t("settings.store.report")}
          />
        </aside>
      </div>
    </section>
  );
}
