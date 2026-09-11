import { ScrollText } from "lucide-react";

import { t } from "../../i18n/t";
import { reportError } from "../../ipc/log";
import { openSettings } from "../../ipc/surface";
import { useSmabar } from "../../store/bar";

/**
 * The bar's only tile while the terms of use are not accepted: it opens the
 * settings on the legal group. Shell chrome like the gear, not a PluginTile —
 * `data-tile-id` is the contract of the context menu and the capture
 * targets, and a tile without a registry entry must not appear there.
 */
export function LegalGateTile() {
  const card = useSmabar((s) => s.appearance.tileChrome === "card");
  const label = t("bar.legal.accept");
  return (
    <button
      className={`${card ? "surface-tile surface-tile-hover" : ""} bar-hover-foreground text-dim flex shrink-0 items-center justify-center gap-2 rounded-sb-s px-3 py-1.5 whitespace-nowrap`}
      data-tile-chrome={card ? "card" : "flat"}
      data-legal-gate
      onClick={(e) => {
        e.stopPropagation();
        void openSettings("legal").catch(reportError);
      }}
      aria-label={label}
    >
      <ScrollText size="1em" aria-hidden="true" />
      <span>{label}</span>
    </button>
  );
}
