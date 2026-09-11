import type { UnlistenFn } from "@tauri-apps/api/event";

import { reportError } from "./log";

/** Cleans up every listener, including ones that resolve after disposal. */
export function cleanupListeners(
  registrations: readonly Promise<UnlistenFn>[],
): UnlistenFn {
  let disposed = false;
  const stops: UnlistenFn[] = [];

  for (const registration of registrations) {
    void registration
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(reportError);
  }

  return () => {
    disposed = true;
    for (const stop of stops) {
      try {
        stop();
      } catch (error) {
        reportError(error);
      }
    }
    stops.length = 0;
  };
}
