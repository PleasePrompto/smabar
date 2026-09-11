import { Check, ExternalLink, TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import type {
  Incompatibility,
  StoreEntry,
  StoreOverview,
} from "../../ipc/store";
import {
  commitUrl,
  formatDate,
  shortCommit,
  stateOf,
  type StoreState,
} from "./storeModel";

/**
 * An external link: a button, so the core opens it in the system browser.
 * The label is short; the URL stays readable as the title and in the
 * accessible name.
 */
export function LinkButton({ url, label }: { url: string; label: string }) {
  return (
    <button
      type="button"
      className="sb-btn sb-btn-ghost settings-store-link"
      title={url}
      aria-label={t("settings.store.openLink").replace("{url}", url)}
      onClick={() => {
        void call("open_url", { url }).catch(reportError);
      }}
    >
      <span>{label}</span>
      <ExternalLink size="1em" aria-hidden="true" />
    </button>
  );
}

function Alert({
  tone,
  children,
}: {
  tone: "warn" | "danger" | "info";
  children: ReactNode;
}) {
  return (
    <div className={`sb-alert sb-alert--${tone}`} role="note">
      <TriangleAlert size="1em" className="sb-alert__icon" aria-hidden="true" />
      <div className="sb-alert__text">{children}</div>
    </div>
  );
}

/**
 * Only facts. Every line names something the store or the catalog actually
 * checked; none of them claims the code is safe, and the wording in the
 * locale files must stay that way.
 */
export function TrustFacts({
  entry,
  overview,
}: {
  entry: StoreEntry;
  overview: StoreOverview;
}) {
  const facts = [t("settings.store.trustPublicSource")];
  if (overview.catalogState !== "unavailable") {
    facts.push(t("settings.store.trustSignature"));
  }
  facts.push(
    entry.repo.license === null
      ? t("settings.store.trustNoLicense")
      : t("settings.store.trustLicense").replace(
          "{license}",
          entry.repo.license,
        ),
  );
  if (entry.installed?.origin === "store" && !entry.installed.modified) {
    facts.push(t("settings.store.trustHash"));
  }
  return (
    <ul className="settings-store-trust" aria-label={t("settings.store.trust")}>
      {facts.map((fact) => (
        <li key={fact}>
          <Check size="1em" aria-hidden="true" />
          {fact}
        </li>
      ))}
    </ul>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}

const OS_LABELS: Record<string, string> = {
  linux: "settings.store.osLinux",
  windows: "settings.store.osWindows",
  macos: "settings.store.osMacos",
};

function osLabel(os: StoreEntry["requires"]["os"]): string {
  if (os.length === 0) return t("settings.store.osAny");
  return os
    .map((name) => {
      const key = OS_LABELS[name];
      return key === undefined ? name : t(key);
    })
    .join(", ");
}

function incompatibilityText(
  reason: Incompatibility,
  entry: StoreEntry,
  overview: StoreOverview,
): string {
  switch (reason) {
    case "os":
      return t("settings.store.incompatibleOs").replace(
        "{os}",
        overview.hostOs ?? "",
      );
    case "minSmabar":
      return t("settings.store.incompatibleMinSmabar")
        .replace("{required}", entry.requires.smabar ?? "")
        .replace("{current}", overview.appVersion);
    case "basePlugin":
      return t("settings.store.incompatibleBasePlugin");
    case "userPlugin":
      return t("settings.store.incompatibleUserPlugin");
    case "blocked":
      return t("settings.store.blockedBy").replace(
        "{reason}",
        entry.blocked?.reason ?? "",
      );
    case "bundledTheme":
      return t("settings.store.incompatibleBundledTheme");
  }
}

/** What is on disk and what the catalog offers on top of it, in words. */
function StateNotes({
  entry,
  state,
}: {
  entry: StoreEntry;
  state: StoreState;
}) {
  const { installed, update } = entry;
  if (installed === null) {
    return entry.blocked === null ? null : (
      <Alert tone="danger">
        {t("settings.store.blockedBy").replace(
          "{reason}",
          entry.blocked.reason,
        )}{" "}
        {t("settings.store.blockedListing")}
      </Alert>
    );
  }
  if (state === "local") {
    return (
      <Alert tone="info">
        <strong>{t("settings.store.stateLocal")}</strong>{" "}
        {t("settings.store.installedLocal")}
      </Alert>
    );
  }
  return (
    <>
      <p className="sb-dim sb-text-s">
        {t("settings.store.installedVersion")
          .replace("{version}", installed.version)
          .replace("{commit}", shortCommit(installed.commit))}
        {installed.deactivated && ` · ${t("settings.store.installedOff")}`}
        {update !== null &&
          ` · ${t("settings.store.updateListed").replace("{version}", update.toVersion)}`}
      </p>
      {installed.blocked !== null && (
        <Alert tone="danger">
          {t("settings.store.blockedBy").replace(
            "{reason}",
            installed.blocked.reason,
          )}{" "}
          {t("settings.store.installedBlocked")}
        </Alert>
      )}
      {installed.modified && (
        <Alert tone="warn">{t("settings.store.installedModified")}</Alert>
      )}
      {update?.contentChanged === true && (
        <Alert tone="warn">
          {t("settings.store.contentChanged").replace(
            "{version}",
            update.toVersion,
          )}
        </Alert>
      )}
    </>
  );
}

/**
 * What a user has to read before the Install button: external programs the
 * plugin needs, why it cannot be installed here, and what is already on
 * disk. Rendered above the readme, never tucked into the sidebar.
 */
export function StoreNotes({
  entry,
  overview,
}: {
  entry: StoreEntry;
  overview: StoreOverview;
}) {
  // A blocked listing gets its own alert below; the reason is not listed twice.
  const reasons = entry.incompatible.filter(
    (reason) => reason !== "blocked" || entry.blocked === null,
  );
  return (
    <>
      {entry.requires.external.length > 0 && (
        <Alert tone="warn">
          <strong>{t("settings.store.external")}</strong>{" "}
          {t("settings.store.externalWarning").replace(
            "{programs}",
            entry.requires.external.join(", "),
          )}
        </Alert>
      )}
      {entry.installed === null && reasons.length > 0 && (
        <ul className="settings-store-reasons">
          {reasons.map((reason) => (
            <li key={reason} className="sb-warn sb-text-s">
              {incompatibilityText(reason, entry, overview)}
            </li>
          ))}
        </ul>
      )}
      <StateNotes entry={entry} state={stateOf(entry)} />
    </>
  );
}

/**
 * The sidebar facts: where the code comes from (repository, the exact
 * commit, the author) and what it needs. Short labels; every link opens
 * through the core.
 */
export function StoreFacts({
  entry,
  language,
}: {
  entry: StoreEntry;
  language: string;
}) {
  return (
    <dl className="settings-store-facts">
      <Fact label={t("settings.store.repository")}>
        <LinkButton url={entry.repo.url} label={entry.repo.nameWithOwner} />
        {entry.repo.archived && (
          <span className="sb-warn">{t("settings.store.archived")}</span>
        )}
      </Fact>
      <Fact label={t("settings.store.sourceCommit")}>
        <LinkButton
          url={commitUrl(entry)}
          label={`${shortCommit(entry.commit)} · ${entry.ref}`}
        />
      </Fact>
      <Fact label={t("settings.store.author")}>
        <LinkButton url={entry.author.url} label={entry.author.login} />
      </Fact>
      {entry.runtime !== null && (
        <Fact label={t("settings.store.runtime")}>
          {t(
            entry.runtime === "python"
              ? "settings.store.runtimePython"
              : "settings.store.runtimeExec",
          )}
        </Fact>
      )}
      <Fact label={t("settings.store.os")}>{osLabel(entry.requires.os)}</Fact>
      <Fact label={t("settings.store.minSmabar")}>
        {entry.requires.smabar ?? t("settings.store.notDeclared")}
      </Fact>
      <Fact label={t("settings.store.license")}>
        {entry.repo.license ?? t("settings.store.notDeclared")}
      </Fact>
      {entry.repo.pushedAt !== null && (
        <Fact label={t("settings.store.pushedAt")}>
          {formatDate(entry.repo.pushedAt, language)}
        </Fact>
      )}
      <Fact label={t("settings.store.updatedAt")}>
        {formatDate(entry.updatedAt, language)}
      </Fact>
    </dl>
  );
}
