/**
 * Declarative micro-charts for plugin HTML. Plugins mark an element with
 * `data-chart="donut"` (data-value, optional data-max) or
 * `data-chart="sparkline"` (data-points="n,n,…"); the shell builds the SVG
 * itself AFTER sanitizing (see ShadowHost). Broken numbers render nothing.
 *
 * Value memory: every plugin render REPLACES the shadow DOM, so a CSS
 * transition can never see the previous value on its own. The maps below
 * remember the last painted value per tile surface (`memoryKey`, provided
 * by ShadowHost) — a rebuilt donut or progress bar starts at its old value
 * and glides to the new one (the transitions live in ui-kit.css and honor
 * prefers-reduced-motion). Without a key (first render, tests) values apply
 * directly.
 */

const SVG_NS = "http://www.w3.org/2000/svg";

/** Radius chosen so the circle's circumference is exactly 100 units —
 * stroke-dasharray/-dashoffset then work in percent. */
const DONUT_RADIUS = 15.915;

const lastDonutOffset = new Map<string, string>();
const lastProgressWidth = new Map<string, string>();
const revisions = new Map<string, number>();
let nextRevision = 0;

function parseNumber(raw: string | null): number | undefined {
  if (raw === null || raw.trim() === "") return undefined;
  const value = Number(raw);
  return Number.isFinite(value) ? value : undefined;
}

/** Sets the target on the next frame after `initial` painted, so the CSS
 * transition interpolates between the two. */
function glide(
  key: string,
  apply: (value: string) => void,
  target: string,
  commit: () => void,
): void {
  const revision = ++nextRevision;
  revisions.set(key, revision);
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      if (revisions.get(key) !== revision) return;
      apply(target);
      commit();
    });
  });
}

function keyedElements(
  root: ParentNode,
  selector: string,
  memoryKey: string | undefined,
  kind: string,
): [Element, string | undefined][] {
  const elements = [...root.querySelectorAll(selector)];
  const named = elements
    .map((el) => (el as HTMLElement).dataset.sbKey?.trim() ?? "")
    .filter((key) => key !== "");
  const unique = new Set(named);
  return elements.map((el) => {
    const name = (el as HTMLElement).dataset.sbKey?.trim() ?? "";
    const identity =
      name !== "" && unique.size === named.length
        ? `key:${name}`
        : elements.length === 1
          ? "single"
          : undefined;
    return [
      el,
      memoryKey === undefined || identity === undefined
        ? undefined
        : `${memoryKey}#${kind}:${identity}`,
    ];
  });
}

function removeStale(
  memory: Map<string, unknown>,
  prefix: string,
  active: Set<string>,
): void {
  for (const key of memory.keys()) {
    if (key.startsWith(prefix) && !active.has(key)) {
      memory.delete(key);
      revisions.delete(key);
    }
  }
}

/** Captures the browser's interpolated value immediately before a plugin DOM
 * replacement, so a mid-transition update continues without a jump. */
export function captureChartMemory(root: ParentNode, memoryKey: string): void {
  for (const [el, key] of keyedElements(
    root,
    '[data-chart="donut"]',
    memoryKey,
    "donut",
  )) {
    if (key === undefined) continue;
    const fill = el.querySelector<SVGCircleElement>("circle.sb-donut-fill");
    if (fill === null) continue;
    const computed =
      fill.ownerDocument.defaultView?.getComputedStyle(fill).strokeDashoffset;
    const shown =
      computed === undefined || computed === ""
        ? fill.style.strokeDashoffset
        : computed;
    if (shown !== "") lastDonutOffset.set(key, shown);
    revisions.set(key, ++nextRevision);
  }
  for (const [el, key] of keyedElements(
    root,
    ".sb-progress > *",
    memoryKey,
    "bar",
  )) {
    if (key === undefined || !(el instanceof HTMLElement)) continue;
    const computed = el.ownerDocument.defaultView?.getComputedStyle(el).width;
    const shown =
      computed === undefined || computed === "" ? el.style.width : computed;
    if (shown !== "" && shown !== "auto") lastProgressWidth.set(key, shown);
    revisions.set(key, ++nextRevision);
  }
}

/** Renders both chart kinds under `root`. Runs POST-sanitize. */
export function enhanceCharts(root: ParentNode, memoryKey?: string): void {
  const donuts = keyedElements(
    root,
    '[data-chart="donut"]',
    memoryKey,
    "donut",
  );
  const donutKeys = new Set<string>();
  for (const [el, key] of donuts) {
    if (key !== undefined) donutKeys.add(key);
    renderDonut(el, key);
  }
  for (const el of root.querySelectorAll('[data-chart="sparkline"]')) {
    renderSparkline(el);
  }
  const bars = keyedElements(root, ".sb-progress > *", memoryKey, "bar");
  const barKeys = new Set<string>();
  for (const [el, key] of bars) {
    if (key !== undefined) barKeys.add(key);
    glideProgress(el as HTMLElement, key);
  }
  if (memoryKey !== undefined) {
    removeStale(lastDonutOffset, `${memoryKey}#donut:`, donutKeys);
    removeStale(lastProgressWidth, `${memoryKey}#bar:`, barKeys);
  }
}

/** Releases value memory when a tile surface is unmounted. */
export function clearChartMemory(memoryKey: string): void {
  removeStale(lastDonutOffset, `${memoryKey}#donut:`, new Set());
  removeStale(lastProgressWidth, `${memoryKey}#bar:`, new Set());
}

/** Ring gauge; the element's own children stay as the center label. */
function renderDonut(el: Element, key?: string): void {
  const value = parseNumber(el.getAttribute("data-value"));
  const max = parseNumber(el.getAttribute("data-max")) ?? 100;
  if (value === undefined || max <= 0) return;
  const ratio = Math.min(Math.max(value / max, 0), 1);
  // dasharray "100 100" shows the first `100 − offset` percent of the path.
  const offset = String(100 - ratio * 100);
  const previous = key === undefined ? undefined : lastDonutOffset.get(key);
  if (key !== undefined && previous === undefined) {
    lastDonutOffset.set(key, offset);
  }
  const doc = el.ownerDocument;

  const svg = doc.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 36 36");
  svg.setAttribute("aria-hidden", "true");

  const track = doc.createElementNS(SVG_NS, "circle");
  const fill = doc.createElementNS(SVG_NS, "circle");
  for (const ring of [track, fill]) {
    ring.setAttribute("cx", "18");
    ring.setAttribute("cy", "18");
    ring.setAttribute("r", String(DONUT_RADIUS));
    ring.setAttribute("fill", "none");
    ring.setAttribute("stroke-width", "3.2");
  }
  track.setAttribute("style", "stroke: var(--sb-chart-track)");
  fill.setAttribute("stroke", "currentColor");
  fill.setAttribute("stroke-linecap", "round");
  fill.setAttribute("stroke-dasharray", "100 100");
  // The circle path starts at 3 o'clock; rotate so the fill grows from 12.
  fill.setAttribute("transform", "rotate(-90 18 18)");
  fill.setAttribute("class", "sb-donut-fill");
  fill.style.strokeDashoffset = previous ?? offset;

  svg.append(track, fill);
  el.prepend(svg);
  if (
    key !== undefined &&
    previous !== undefined &&
    Number.parseFloat(previous) !== Number(offset)
  ) {
    glide(
      key,
      (next) => {
        fill.style.strokeDashoffset = next;
      },
      offset,
      () => {
        lastDonutOffset.set(key, offset);
      },
    );
  }
}

/** Restarts a progress bar at its previously painted width, then glides to
 * the width the plugin rendered (`.sb-progress > *` carries the transition). */
function glideProgress(el: HTMLElement, key?: string): void {
  const target = el.style.width;
  if (key === undefined || target === "") return;
  const previous = lastProgressWidth.get(key);
  if (previous === undefined) {
    lastProgressWidth.set(key, target);
    return;
  }
  if (previous === target) return;
  el.style.width = previous;
  glide(
    key,
    (next) => {
      el.style.width = next;
    },
    target,
    () => {
      lastProgressWidth.set(key, target);
    },
  );
}

/** Normalized polyline over the data points; replaces the element's content. */
function renderSparkline(el: Element): void {
  const values: number[] = [];
  for (const raw of (el.getAttribute("data-points") ?? "").split(",")) {
    const point = parseNumber(raw);
    if (point === undefined) return; // one broken number → no chart
    values.push(point);
  }
  if (values.length < 2) return;

  const width = 100;
  const height = 32;
  const pad = 2;
  const min = Math.min(...values);
  const span = Math.max(...values) - min || 1;
  const coords = values
    .map((value, index) => {
      const x = (index / (values.length - 1)) * width;
      const y = height - pad - ((value - min) / span) * (height - 2 * pad);
      return `${String(x)},${String(y)}`;
    })
    .join(" ");

  const doc = el.ownerDocument;
  const svg = doc.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", `0 0 ${String(width)} ${String(height)}`);
  svg.setAttribute("preserveAspectRatio", "none");
  svg.setAttribute("aria-hidden", "true");
  const line = doc.createElementNS(SVG_NS, "polyline");
  line.setAttribute("points", coords);
  line.setAttribute("fill", "none");
  line.setAttribute("stroke", "currentColor");
  line.setAttribute("stroke-width", "2");
  line.setAttribute("stroke-linecap", "round");
  line.setAttribute("stroke-linejoin", "round");
  line.setAttribute("vector-effect", "non-scaling-stroke");
  svg.appendChild(line);
  el.replaceChildren(svg);
}
