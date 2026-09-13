import { uiLog } from "../ipc/log";
import { isKitClass } from "./kitSheet";
import type { SanitizerDrop } from "./sanitize";

/**
 * Turns one render's sanitizer drops into a single entry in the PLUGIN's own
 * log — the place a plugin author's agent already looks.
 *
 * Silently dropped markup used to leave "nothing happens" as the only
 * symptom, with no way to find out why. Reporting it there closes that loop
 * without the author having to know the shell exists.
 */

/** `kind|tile key` → every problem of that kind reported for the surface. */
const reported = new Map<string, Set<string>>();

/**
 * Distinct problems remembered per slot. Lint problems carry rendered text,
 * so a value that changes every render would otherwise grow this memory and
 * the plugin log without limit; after the cap the slot stays silent until a
 * clean render re-arms it.
 */
export const MAX_PROBLEMS_PER_SLOT = 32;

/** Test seam: forget what has already been reported. */
export function resetMarkupReports(): void {
  reported.clear();
}

/**
 * Remembers `items` as reported for one kind of problem on one tile
 * surface and returns the ones not reported before. An empty `items` is a
 * render without that kind of problem: it forgets the memory, so the same
 * problem is reported again if a later render brings it back — a fixed
 * problem goes silent, a reintroduced one does not.
 */
export function rememberProblems(
  kind: string,
  key: string,
  items: readonly string[],
): string[] {
  const slot = `${kind}|${key}`;
  if (items.length === 0) {
    reported.delete(slot);
    return [];
  }
  const seen = reported.get(slot) ?? new Set<string>();
  reported.set(slot, seen);
  const fresh: string[] = [];
  for (const item of items) {
    if (seen.has(item)) continue;
    if (seen.size >= MAX_PROBLEMS_PER_SLOT) break;
    seen.add(item);
    fresh.push(item);
  }
  return fresh;
}

/** Groups repeated removals and keeps their order deterministic. */
function summarize(
  drops: SanitizerDrop[],
): { problem: string; what: string; count: number; reason: string }[] {
  const counts = new Map<string, { count: number; reason: string }>();
  for (const drop of drops) {
    const problem = `${drop.what}\u0000${drop.reason}`;
    const seen = counts.get(problem);
    if (seen === undefined) {
      counts.set(problem, { count: 1, reason: drop.reason });
    } else {
      seen.count += 1;
    }
  }
  return [...counts.entries()]
    .map(([problem, { count, reason }]) => ({
      problem,
      what: problem.slice(0, problem.indexOf("\u0000")),
      count,
      reason,
    }))
    .sort(
      (a, b) =>
        a.what.localeCompare(b.what) || a.reason.localeCompare(b.reason),
    );
}

/**
 * Reports what the sanitizer removed, at most once per individual problem
 * per tile. Plugins re-render every second; the same problem must not
 * produce a line every second.
 */
export function reportMarkupDrops(
  pluginId: string,
  tileId: string,
  target: string,
  drops: SanitizerDrop[],
): void {
  const key = `${pluginId}/${tileId}/${target}`;
  const all = summarize(drops);
  const unseen = rememberProblems(
    "drops",
    key,
    all.map(({ problem }) => problem),
  );
  const details = all.filter(({ problem }) => unseen.includes(problem));
  if (details.length === 0) return;
  const summary = details
    .map(({ what, count }) => (count > 1 ? `${what} (${String(count)})` : what))
    .join(", ");
  uiLog("warn", `markup removed from "${tileId}" (${target}): ${summary}`, {
    pluginId,
    fields: {
      tileId,
      target,
      dropped: details.map(({ what, count, reason }) => ({
        what,
        count,
        reason,
      })),
    },
  });
}

/**
 * Reports unsupported `sb-*` classes without changing the plugin's markup.
 * The fragment is checked before behaviour enhancers can add shell-owned
 * implementation classes.
 */
export function reportUnknownKitClasses(
  pluginId: string,
  tileId: string,
  target: string,
  markup: DocumentFragment,
): void {
  const unknownClasses = new Set<string>();
  for (const element of markup.querySelectorAll("[class]")) {
    for (const className of element.classList) {
      if (className.startsWith("sb-") && !isKitClass(className)) {
        unknownClasses.add(className);
      }
    }
  }

  const key = `${pluginId}/${tileId}/${target}`;
  const sortedClasses = [...unknownClasses].sort((a, b) => a.localeCompare(b));
  const newClasses = rememberProblems("classes", key, sortedClasses);
  if (newClasses.length === 0) return;
  const summary = newClasses.join(", ");
  uiLog(
    "warn",
    `unknown sb-* classes in "${tileId}" (${target}): ${summary}. These classes have no matching rule in the loaded UI kit; use ui_kit to choose supported classes or remove the sb- prefix.`,
    {
      pluginId,
      fields: { tileId, target, unknownClasses: newClasses },
    },
  );
}
