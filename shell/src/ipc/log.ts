import { call } from "./call";

/**
 * The shell's logging path.
 *
 * Before this existed, every failure in the webview went to
 * `globalThis.reportError` — the browser console and nothing else. Not the
 * log file, not MCP, not the user. `console.*` is banned in shell code for
 * exactly that reason: it is not a log. This is the sanctioned replacement,
 * and it reaches `~/.smabar/logs/` like everything else.
 *
 * `pluginId` decides WHERE: with it the entry lands in that plugin's own log
 * (source `"shell"`), which is where a plugin author's agent already looks.
 */

export type UiLogLevel = "debug" | "info" | "warn" | "error";

/** Repeats of the same message inside this window are dropped. */
export const DEDUPE_WINDOW_MS = 30_000;

/** Once the command itself fails, reporting stops for good. */
let shipping = true;
/** Message key → timestamp of the last shipment. */
const lastSent = new Map<string, number>();

/** Test seam: forget the dedupe state and re-arm shipping. */
export function resetUiLog(): void {
  shipping = true;
  lastSent.clear();
}

/** True when this exact message may be sent again. */
function allow(key: string, now: number): boolean {
  const previous = lastSent.get(key);
  if (previous !== undefined && now - previous < DEDUPE_WINDOW_MS) return false;
  lastSent.set(key, now);
  // The map only ever holds distinct messages; a shell that produces enough
  // of those to matter has a bigger problem than memory.
  return true;
}

/**
 * Sends one entry to the core. Never throws and never reports its own
 * failure through this same path — that would be an error loop, so a failed
 * send disables shipping instead.
 */
export function uiLog(
  level: UiLogLevel,
  message: string,
  options: { fields?: Record<string, unknown>; pluginId?: string } = {},
): void {
  if (!shipping) return;
  const key = `${options.pluginId ?? ""}|${level}|${message}`;
  if (!allow(key, Date.now())) return;
  void call("ui_log", {
    level,
    message,
    fields: options.fields ?? null,
    pluginId: options.pluginId ?? null,
  }).catch(() => {
    shipping = false;
  });
}

/** Turns anything a `catch` can receive into one readable line. */
export function describeError(error: unknown): string {
  try {
    if (error instanceof Error) {
      return error.stack ?? `${error.name}: ${error.message}`;
    }
    if (typeof error === "string") return error;
    const serialized: unknown = JSON.stringify(error);
    return typeof serialized === "string" ? serialized : String(error);
  } catch {
    try {
      return String(error);
    } catch {
      return "[unprintable thrown value]";
    }
  }
}

/**
 * The line a settings row may show: an Error's message alone (its stack is
 * for the log), a rejected command's string as is.
 */
export function visibleError(error: unknown): string {
  return error instanceof Error ? error.message : describeError(error);
}

/**
 * Drop-in replacement for the `reportError` global used across the shell:
 * keeps the devtools behaviour AND puts the failure in the central log.
 */
export function reportError(error: unknown): void {
  globalThis.reportError(error);
  uiLog("error", describeError(error));
}

/**
 * Catches what nobody handled. Without this an exception in a React render
 * or a rejected promise leaves no trace outside the console.
 */
export function installGlobalErrorReporting(target: Window = window): void {
  target.addEventListener("error", (event: ErrorEvent) => {
    uiLog("error", describeError(event.error ?? event.message), {
      fields: { source: event.filename, line: event.lineno },
    });
  });
  target.addEventListener(
    "unhandledrejection",
    (event: PromiseRejectionEvent) => {
      uiLog("error", describeError(event.reason), {
        fields: { kind: "unhandledrejection" },
      });
    },
  );
}
