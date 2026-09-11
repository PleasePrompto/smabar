import { useEffect, useState } from "react";

import { safeIntlLocale, t } from "../../i18n/t";
import { call } from "../../ipc/call";
import {
  acceptLegal,
  declineLegal,
  legalStatus,
  type LegalStatus,
} from "../../ipc/legal";
import { reportError, visibleError } from "../../ipc/log";
import { useSmabar } from "../../store/bar";
import {
  Choice,
  ChoiceGrid,
  ConfirmRow,
  SettingRow,
  SettingsSection,
} from "./controls";
import { LanguageChoice } from "./LanguageChoice";
import { StoreReadme } from "./StoreReadme";

type DocumentId = "terms" | "license" | "privacy";

const DOCUMENTS: readonly { id: DocumentId; labelKey: string }[] = [
  { id: "terms", labelKey: "settings.legal.tabTerms" },
  { id: "license", labelKey: "settings.legal.tabLicense" },
  { id: "privacy", labelKey: "settings.legal.tabPrivacy" },
];

/**
 * A `YYYY-MM-DD` front-matter date as a long date. Formatted in UTC: the
 * string parses as UTC midnight, and a local-time format would show the day
 * before it anywhere west of Greenwich. Anything else is shown as it is.
 */
function formatDay(day: string, language: string): string {
  const parsed = Date.parse(day);
  if (Number.isNaN(parsed)) return day;
  return new Intl.DateTimeFormat(safeIntlLocale(language), {
    dateStyle: "long",
    timeZone: "UTC",
  }).format(parsed);
}

/** What the section says above the documents: why they are shown, or when they were accepted. */
function hintFor(status: LegalStatus, language: string): string | null {
  if (status.required) {
    return t(
      status.acceptedAt === null
        ? "settings.legal.hint"
        : "settings.legal.hintUpdated",
    );
  }
  if (status.acceptedAt === null) return null;
  const acceptedAt = new Intl.DateTimeFormat(safeIntlLocale(language), {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(status.acceptedAt);
  return t("settings.legal.acceptedOn")
    .replace("{date}", acceptedAt)
    .replace("{version}", formatDay(status.termsVersion, language));
}

/**
 * The legal group (ADR 0017), the only one the panel shows while the bundled
 * terms are not accepted. Afterwards the texts live in a collapsed block
 * under System, so the notice stops shouting once it has done its job.
 */
export function LegalTab() {
  return (
    <SettingsSection title={t("settings.group.legal")}>
      <LegalDocuments />
    </SettingsSection>
  );
}

/**
 * Terms of use, license and privacy notice as the core rendered them. While
 * the bundled terms are not accepted it also carries the two ways out of the
 * gate — accepting records the acceptance, declining quits smabar after an
 * inline confirmation — and the language choice, because the System tab is
 * out of reach then. The privacy notice is shown, never asked for.
 */
export function LegalDocuments() {
  const language = useSmabar((state) => state.language);
  const [status, setStatus] = useState<LegalStatus | null>(null);
  const [shown, setShown] = useState<DocumentId>("terms");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmDecline, setConfirmDecline] = useState(false);
  // The languages to offer while gated: the System tab is out of reach then.
  const [languages, setLanguages] = useState<readonly string[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    legalStatus()
      .then((next) => {
        if (!cancelled) setStatus(next);
      })
      .catch(reportError);
    call<{ languages: string[] }>("get_system_settings")
      .then((system) => {
        if (!cancelled) setLanguages(system.languages);
      })
      .catch(reportError);
    return () => {
      cancelled = true;
    };
  }, []);

  const accept = async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await acceptLegal());
    } catch (caught: unknown) {
      setError(
        t("settings.legal.error").replace("{error}", visibleError(caught)),
      );
    } finally {
      setBusy(false);
    }
  };

  const hint = status === null ? null : hintFor(status, language);
  const doc = status?.[shown] ?? null;

  return status === null ? (
    <p className="settings-help">{t("settings.legal.loading")}</p>
  ) : (
    <div className="flex flex-col gap-3">
      {hint !== null && <p className="settings-help">{hint}</p>}
      {status.required && (
        <SettingRow label={t("settings.system.language")}>
          <LanguageChoice languages={languages ?? [language]} />
        </SettingRow>
      )}
      <ChoiceGrid label={t("settings.legal.tabs")}>
        {DOCUMENTS.map((entry) => (
          <Choice
            key={entry.id}
            label={t(entry.labelKey)}
            active={entry.id === shown}
            onClick={() => {
              setShown(entry.id);
            }}
          />
        ))}
      </ChoiceGrid>
      {doc?.updated != null && (
        <p className="sb-faint sb-text-xs">
          {t("settings.legal.asOf").replace(
            "{date}",
            formatDay(doc.updated, language),
          )}
        </p>
      )}
      {doc !== null && (
        <div className="settings-store-readme">
          <StoreReadme html={doc.html} />
        </div>
      )}
      {status.required && error !== null && (
        <div className="sb-alert sb-alert--danger" role="alert">
          <div className="sb-alert__text">{error}</div>
        </div>
      )}
      {status.required &&
        (confirmDecline ? (
          <ConfirmRow
            label={t("settings.legal.decline")}
            question={t("settings.legal.declineQuestion")}
            action={t("settings.legal.decline")}
            onCancel={() => {
              setConfirmDecline(false);
            }}
            onConfirm={() => {
              void declineLegal().catch(reportError);
            }}
          />
        ) : (
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              className="sb-btn sb-btn-primary"
              disabled={busy}
              onClick={() => {
                void accept();
              }}
            >
              {t("settings.legal.accept")}
            </button>
            <button
              type="button"
              className="sb-btn sb-btn-danger"
              disabled={busy}
              onClick={() => {
                setConfirmDecline(true);
              }}
            >
              {t("settings.legal.decline")}
            </button>
          </div>
        ))}
    </div>
  );
}
