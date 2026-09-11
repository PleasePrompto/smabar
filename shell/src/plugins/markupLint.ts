import { uiLog } from "../ipc/log";
import { rememberProblems } from "./markupReport";

/**
 * Lints plugin markup for the mistakes the kit contract forbids in prose and
 * nothing used to catch: spacing and type set inline, icon-only controls
 * without a name, raw locale keys rendered as text, KPI values that cannot
 * fit their cell. Each finding lands in the PLUGIN's own log — the place a
 * plugin author's agent already looks — with the fix spelled out.
 *
 * Nothing here changes the markup: a plugin that ignores the report renders
 * exactly as before. The lint runs before the behaviour enhancers, so it
 * sees the plugin's own attributes and not the shell's.
 */

export type LintKind =
  "inlineStyle" | "unnamedControl" | "localeKey" | "kpiValue";

export interface LintFinding {
  kind: LintKind;
  /** The element or text the finding is about, as the report names it. */
  what: string;
  /** For inline styles: the offending property. */
  property?: string;
}

const XHTML = "http://www.w3.org/1999/xhtml";
/** Properties the kit's automatic rhythm and type scale own. */
const SPACING_PROPERTY =
  /^(?:margin|padding)(?:-|$)|^(?:gap|row-gap|column-gap)$/;
const FLEX_OR_GRID = /^(?:inline-)?(?:flex|grid)$/;
/** A dotted identifier the way locale keys look: `weather.title`, `a.b.c`. */
const LOCALE_KEY = /^[a-z][a-z0-9_]*(?:\.[A-Za-z0-9_]+)+$/;
/** Elements whose text is literal by nature — never a missed translation. */
const LITERAL_TEXT = "code, kbd, pre, samp, var, a";
/** Longer than this and an `sb-kpi-value` overflows its half-width cell. */
const KPI_VALUE_MAX_CHARS = 12;

function describe(element: Element): string {
  const tag = element.localName;
  const action = element.getAttribute("data-action");
  if (action !== null) return `${tag}[data-action="${action}"]`;
  const id = element.getAttribute("id");
  if (id !== null) return `${tag}#${id}`;
  return tag;
}

function inlineStyleFindings(
  markup: DocumentFragment,
  target: string,
): LintFinding[] {
  const findings: LintFinding[] = [];
  for (const element of markup.querySelectorAll("[style]")) {
    if (element.namespaceURI !== XHTML) continue;
    const declarations = (element.getAttribute("style") ?? "").split(";");
    for (const declaration of declarations) {
      const colon = declaration.indexOf(":");
      if (colon === -1) continue;
      const property = declaration.slice(0, colon).trim().toLowerCase();
      const value = declaration
        .slice(colon + 1)
        .trim()
        .toLowerCase();
      const onTile =
        target === "tile" &&
        (element.classList.contains("sb-tile") ||
          element.parentNode === markup);
      const flagged =
        SPACING_PROPERTY.test(property) ||
        property === "font-size" ||
        property === "text-align" ||
        (property === "display" && FLEX_OR_GRID.test(value)) ||
        (onTile && (property === "width" || property === "min-width"));
      if (flagged) {
        findings.push({
          kind: "inlineStyle",
          what: `${element.localName}[style: ${property}]`,
          property,
        });
      }
    }
  }
  return findings;
}

function unnamedControlFindings(markup: DocumentFragment): LintFinding[] {
  const findings: LintFinding[] = [];
  const seen = new Set<Element>();
  for (const element of markup.querySelectorAll("button, [data-action]")) {
    if (seen.has(element)) continue;
    seen.add(element);
    if (element.textContent.trim() !== "") continue;
    const named = ["title", "aria-label", "aria-labelledby"].some(
      (name) => (element.getAttribute(name) ?? "").trim() !== "",
    );
    if (named) continue;
    const altText =
      element.querySelector("img[alt]")?.getAttribute("alt") ?? "";
    if (altText.trim() !== "") continue;
    if (element.querySelector("[data-lucide], svg, img") === null) continue;
    findings.push({ kind: "unnamedControl", what: describe(element) });
  }
  return findings;
}

function localeKeyFindings(
  markup: DocumentFragment,
  pluginId: string,
  tileId: string,
): LintFinding[] {
  const findings: LintFinding[] = [];
  const walker = markup.ownerDocument.createTreeWalker(
    markup,
    NodeFilter.SHOW_TEXT,
  );
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    const text = (node.textContent ?? "").trim();
    if (!LOCALE_KEY.test(text)) continue;
    if (node.parentElement?.closest(LITERAL_TEXT) !== null) continue;
    const segments = text.split(".");
    // Every segment after the first carries a letter: "v1.2.3" is a
    // version, "weather.day.mon" a key.
    if (segments.slice(1).some((segment) => !/[A-Za-z]/.test(segment)))
      continue;
    const owned = segments[0] === pluginId || segments[0] === tileId;
    if (segments.length < 3 && !owned) continue;
    findings.push({ kind: "localeKey", what: text });
  }
  return findings;
}

function kpiValueFindings(markup: DocumentFragment): LintFinding[] {
  const findings: LintFinding[] = [];
  for (const element of markup.querySelectorAll(".sb-kpi-value")) {
    const text = element.textContent.trim();
    if (text.length > KPI_VALUE_MAX_CHARS) {
      findings.push({
        kind: "kpiValue",
        what: `"${text}" (${String(text.length)} chars)`,
      });
    }
  }
  return findings;
}

/** Every finding of every kind, in document order per kind. */
export function lintMarkup(
  markup: DocumentFragment,
  target: string,
  pluginId: string,
  tileId: string,
): LintFinding[] {
  return [
    ...inlineStyleFindings(markup, target),
    ...unnamedControlFindings(markup),
    ...localeKeyFindings(markup, pluginId, tileId),
    ...kpiValueFindings(markup),
  ];
}

/** The fix for each group of inline properties actually found. */
function inlineStyleFixes(properties: Set<string>): string[] {
  const fixes: string[] = [];
  if ([...properties].some((property) => SPACING_PROPERTY.test(property))) {
    fixes.push(
      "margin/padding/gap → spacing is automatic, use sb-stack, sb-inline, sb-tight or sb-flush",
    );
  }
  if (properties.has("font-size"))
    fixes.push("font-size → sb-text-xs/-s/-m/-l/-xl/-hero");
  if (properties.has("text-align")) fixes.push("text-align → sb-center");
  if (properties.has("display")) {
    fixes.push(
      "display:flex/grid → sb-inline, sb-stack or sb-grid with --sb-grid-cols",
    );
  }
  if (properties.has("width") || properties.has("min-width")) {
    fixes.push(
      "width/min-width on the tile → the tile hugs its content (sb-mono, data-marquee, data-rotator; see ui_kit tileSizing)",
    );
  }
  return fixes;
}

const MESSAGE: Record<
  LintKind,
  (where: string, items: string[], findings: LintFinding[]) => string
> = {
  inlineStyle: (where, items, findings) =>
    `inline style in ${where}: ${items.join(", ")} — ${inlineStyleFixes(
      new Set(findings.map((finding) => finding.property ?? "")),
    ).join(
      "; ",
    )}. style= is fine for a data value (a progress width, --sb-chart-value, --sb-grid-cols).`,
  unnamedControl: (where, items) =>
    `unnamed icon-only controls in ${where}: ${items.join(", ")} — add title="…" AND aria-label="…" with the same text; the shell turns title into its themed tooltip, so without one the control has no name and no tooltip.`,
  localeKey: (where, items) =>
    `raw locale keys rendered as text in ${where}: ${items.join(", ")} — t() ran before initialize or the key is missing: render from @app.on_ready and add the key to locales/en.json (wrap literal dotted text in <code> to silence this).`,
  kpiValue: (where, items) =>
    `long KPI values in ${where}: ${items.join(", ")} — sb-kpi-value sits in a 2-column grid at the xl type step; round or drop units, or use one sb-stat instead.`,
};

const KINDS: LintKind[] = [
  "inlineStyle",
  "unnamedControl",
  "localeKey",
  "kpiValue",
];

/**
 * Lints one render and reports each kind of finding once per distinct
 * problem per tile surface; a render without findings of a kind re-arms
 * that kind, so a problem that comes back is reported again.
 */
export function reportMarkupLint(
  pluginId: string,
  tileId: string,
  target: string,
  markup: DocumentFragment,
): void {
  const findings = lintMarkup(markup, target, pluginId, tileId);
  const key = `${pluginId}/${tileId}/${target}`;
  for (const kind of KINDS) {
    const ofKind = findings.filter((finding) => finding.kind === kind);
    const fresh = rememberProblems(
      kind,
      key,
      ofKind.map((finding) => finding.what),
    );
    if (fresh.length === 0) continue;
    const freshFindings = ofKind.filter((finding) =>
      fresh.includes(finding.what),
    );
    uiLog(
      "warn",
      MESSAGE[kind](`"${tileId}" (${target})`, fresh, freshFindings),
      {
        pluginId,
        fields: { tileId, target, kind, findings: fresh },
      },
    );
  }
}
