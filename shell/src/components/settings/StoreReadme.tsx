import type { MouseEvent } from "react";

import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";

/**
 * The README as the core rendered it. The markup is the converter's own —
 * raw HTML from the author never reaches it — so it is set as innerHTML.
 * Links leave through the core: navigating the settings webview to GitHub
 * would replace the whole panel, so every click on one is intercepted.
 */
export function StoreReadme({ html }: { html: string }) {
  const openLink = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const link = target.closest("a[href]");
    if (link === null) return;
    event.preventDefault();
    const url = link.getAttribute("href");
    if (url !== null) void call("open_url", { url }).catch(reportError);
  };
  return (
    <div
      className="settings-readme"
      dangerouslySetInnerHTML={{ __html: html }}
      onClick={openLink}
      onAuxClick={openLink}
    />
  );
}
