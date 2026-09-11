import type { ReactNode } from "react";

import { t } from "../../i18n/t";
import type { StoreEntry } from "../../ipc/store";
import { badgeFor, stateOf, type BadgeTone } from "./storeModel";
import type { PluginProvenance } from "./pluginListModel";

const TONE_CLASS: Record<BadgeTone, string> = {
  neutral: "sb-badge",
  ok: "sb-badge sb-badge-success",
  warn: "sb-badge sb-badge-warning",
  danger: "sb-badge sb-badge-danger",
  accent: "sb-badge sb-badge-accent",
};

function Badge({
  tone,
  title,
  children,
}: {
  tone: BadgeTone;
  /** The longer fact behind a short label — a block's reason, say. */
  title?: string;
  children: ReactNode;
}) {
  return (
    <span className={TONE_CLASS[tone]} title={title}>
      {children}
    </span>
  );
}

/** The one-word state of a Community Store listing; nothing for "available". */
export function StoreStateBadge({ entry }: { entry: StoreEntry }) {
  const badge = badgeFor(stateOf(entry));
  if (badge === null) return null;
  const reason = entry.installed?.blocked?.reason ?? entry.blocked?.reason;
  return (
    <Badge
      tone={badge.tone}
      title={
        reason === undefined
          ? undefined
          : t("settings.store.blockedBy").replace("{reason}", reason)
      }
    >
      {t(badge.key).replace("{version}", entry.update?.toVersion ?? "")}
    </Badge>
  );
}

/**
 * Where an installed plugin came from, on its row of the Installed list —
 * and, for a Community Plugin, what the store knows beyond that: a newer
 * version, a local modification, a block.
 */
export function OriginBadge({
  provenance,
}: {
  provenance: PluginProvenance | null;
}) {
  if (provenance === null) return null;
  const origin =
    provenance.origin === "community"
      ? [t("settings.plugins.originCommunity"), provenance.version]
          .filter((part) => part !== null)
          .join(" ")
      : t(
          provenance.origin === "base"
            ? "settings.plugins.originBase"
            : "settings.plugins.originUser",
        );
  return (
    <>
      <Badge tone="neutral">{origin}</Badge>
      {provenance.blocked !== null ? (
        <Badge
          tone="danger"
          title={t("settings.plugins.originBlocked").replace(
            "{reason}",
            provenance.blocked,
          )}
        >
          {t("settings.store.stateBlocked")}
        </Badge>
      ) : (
        provenance.update !== null && (
          <Badge
            tone="accent"
            title={t("settings.plugins.originUpdate").replace(
              "{version}",
              provenance.update,
            )}
          >
            {t("settings.store.stateUpdate").replace(
              "{version}",
              provenance.update,
            )}
          </Badge>
        )
      )}
      {provenance.modified && (
        <Badge tone="warn">{t("settings.plugins.originModified")}</Badge>
      )}
    </>
  );
}
