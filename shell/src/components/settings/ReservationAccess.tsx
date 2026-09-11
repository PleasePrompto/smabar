import { useEffect, useState } from "react";

import { t } from "../../i18n/t";
import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";
import { SettingRow } from "./controls";

type Access = "ready" | "permissionRequired";

/** macOS requires Accessibility access before it can keep other windows clear. */
export function ReservationAccess() {
  const [access, setAccess] = useState<Access | null>(null);
  const [failed, setFailed] = useState(false);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    let active = true;
    const refresh = () => {
      void call<Access>("get_reservation_status")
        .then((next) => {
          if (active) {
            setAccess(next);
            setFailed(false);
          }
        })
        .catch((error: unknown) => {
          if (active) setFailed(true);
          reportError(error);
        });
    };
    refresh();
    window.addEventListener("focus", refresh);
    return () => {
      active = false;
      window.removeEventListener("focus", refresh);
    };
  }, []);

  if (access !== "permissionRequired" && !failed) return null;

  return (
    <SettingRow
      label={t("settings.behavior.accessibility")}
      description={t("settings.behavior.accessibilityDescription")}
      wide
    >
      {failed && (
        <p className="sb-error" role="alert">
          {t("settings.behavior.accessibilityFailed")}
        </p>
      )}
      <button
        type="button"
        className="sb-btn sb-btn-ghost"
        disabled={pending}
        onClick={() => {
          setPending(true);
          void call<Access>("request_reservation_access")
            .then((next) => {
              setAccess(next);
              setFailed(false);
            })
            .catch((error: unknown) => {
              setFailed(true);
              reportError(error);
            })
            .finally(() => {
              setPending(false);
            });
        }}
      >
        {t("settings.behavior.grantAccessibility")}
      </button>
    </SettingRow>
  );
}
