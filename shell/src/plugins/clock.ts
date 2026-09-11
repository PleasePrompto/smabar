/**
 * Clocks for plugin HTML, so a clock plugin never has to push a frame per
 * second. A plugin that re-rendered at 1 Hz would re-sanitize and rebuild its
 * whole shadow tree every second, throwing away form state, carousel position
 * and focus with it. The plugin renders once and idles; the shell keeps time.
 *
 * Two conventions, both applied POST-sanitize (see ShadowHost):
 *   `data-clock-text="<IANA zone>"` + optional `data-clock-format`
 *       — text that ticks: time (default), date, weekday, week, offset,
 *         unix (seconds since the epoch, for copy buttons and agents).
 *   `data-clock="<IANA zone>"`
 *       — an analog face. Its hands are CSS animations with a negative
 *         `animation-delay` as the start offset, so they sweep smoothly
 *         without a single JS tick; only the offset is recomputed, and only
 *         when the page becomes visible again. Under
 *         `prefers-reduced-motion` the kit pauses that animation, so the
 *         offset is re-seeded every second instead and the face steps.
 *
 * An empty zone means the system zone.
 */

import { safeIntlLocale } from "../i18n/t";

const SVG_NS = "http://www.w3.org/2000/svg";
const FORMATTER_CACHE_LIMIT = 64;
const formatters = new Map<string, Intl.DateTimeFormat>();

/** Hand periods in seconds; also the CSS animation durations. */
const SECOND_PERIOD = 60;
const MINUTE_PERIOD = 3600;
const HOUR_PERIOD = 43200;

export type ClockFormat =
  "time" | "date" | "weekday" | "week" | "offset" | "unix";

/** Wall-clock parts of `date` in `timeZone`; the system zone when empty. */
export function zonedParts(
  date: Date,
  timeZone: string,
): { hours: number; minutes: number; seconds: number } {
  const parts = partsIn(date, timeZone, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
  });
  return {
    hours: Number(parts.hour ?? "0") % 24,
    minutes: Number(parts.minute ?? "0"),
    seconds: Number(parts.second ?? "0"),
  };
}

/**
 * Seconds elapsed since local midnight in `timeZone` — the single offset the
 * three hand animations are derived from.
 */
export function secondsSinceMidnight(date: Date, timeZone: string): number {
  const { hours, minutes, seconds } = zonedParts(date, timeZone);
  return hours * 3600 + minutes * 60 + seconds;
}

/** ISO-8601 calendar week (1-53) of `date` in `timeZone`. */
export function isoWeek(date: Date, timeZone: string): number {
  const parts = partsIn(date, timeZone, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
  // Work in UTC on the zone's calendar date so DST cannot shift the day.
  const day = Date.UTC(
    Number(parts.year ?? "1970"),
    Number(parts.month ?? "1") - 1,
    Number(parts.day ?? "1"),
  );
  const target = new Date(day);
  // ISO weeks are Monday-based and belong to the year of their Thursday.
  const isoDay = (target.getUTCDay() + 6) % 7;
  target.setUTCDate(target.getUTCDate() - isoDay + 3);
  const firstThursday = Date.UTC(target.getUTCFullYear(), 0, 4);
  const firstIsoDay = (new Date(firstThursday).getUTCDay() + 6) % 7;
  const weekOneMonday = firstThursday - firstIsoDay * 86_400_000;
  return Math.round((target.getTime() - weekOneMonday) / (7 * 86_400_000)) + 1;
}

/** UTC offset of `timeZone` at `date`, e.g. "UTC+2" or "UTC−3:30". */
export function utcOffset(date: Date, timeZone: string): string {
  const name = partsIn(date, timeZone, {
    timeZoneName: "longOffset",
  }).timeZoneName;
  // longOffset yields "GMT+02:00"; trim to the shortest exact form.
  const match = /GMT([+-])(\d{2}):(\d{2})/.exec(name ?? "");
  if (match === null) return "UTC±0";
  const [, sign, hours, minutes] = match;
  const hour = Number(hours ?? "0");
  const minute = Number(minutes ?? "0");
  if (hour === 0 && minute === 0) return "UTC±0";
  const body = minute === 0 ? String(hour) : `${String(hour)}:${minutes ?? ""}`;
  return `UTC${sign === "-" ? "−" : "+"}${body}`;
}

/** The text a `data-clock-text` node should currently show. */
export function clockText(
  date: Date,
  timeZone: string,
  format: ClockFormat,
  language: string,
  showSeconds = true,
): string {
  const locale = safeIntlLocale(language);
  const zone = timeZone === "" ? undefined : timeZone;
  switch (format) {
    case "date":
      return formatter(locale, {
        timeZone: zone,
        day: "numeric",
        month: "long",
      }).format(date);
    case "weekday":
      return formatter(locale, {
        timeZone: zone,
        weekday: "long",
      }).format(date);
    case "week":
      return String(isoWeek(date, timeZone));
    case "offset":
      return utcOffset(date, timeZone);
    case "unix":
      return String(Math.floor(date.getTime() / 1000));
    default:
      return formatter(locale, {
        timeZone: zone,
        hour: "2-digit",
        minute: "2-digit",
        second: showSeconds ? "2-digit" : undefined,
        hourCycle: "h23",
      }).format(date);
  }
}

/**
 * Renders analog faces and starts the text tick under `root`. The returned
 * function stops everything again (ShadowHost calls it on teardown).
 */
export function enhanceClocks(
  root: ParentNode,
  now = () => new Date(),
): () => void {
  const faces = [...root.querySelectorAll("[data-clock]")];
  for (const face of faces) renderFace(face, now());

  const texts = [...root.querySelectorAll("[data-clock-text]")];
  const tick = () => {
    const date = now();
    for (const node of texts) {
      node.textContent = clockText(
        date,
        node.getAttribute("data-clock-text") ?? "",
        readFormat(node.getAttribute("data-clock-format")),
        node.getAttribute("data-clock-lang") ?? "en",
        node.getAttribute("data-clock-seconds") !== "false",
      );
    }
  };

  // With reduced motion the hands' animation is paused (ui-kit.css), so the
  // face only shows the time it was seeded with; re-seed it every second.
  const reducedMotion = reducedMotionQuery();
  const stepFaces = () => {
    if (reducedMotion?.matches !== true) return;
    const date = now();
    for (const face of faces) seedHands(face, date);
  };

  let timer: ReturnType<typeof setTimeout> | undefined;
  const schedule = () => {
    tick();
    stepFaces();
    // Align to the next second boundary instead of drifting on a fixed
    // interval — a clock that skips or repeats a second reads as broken.
    timer = setTimeout(schedule, 1000 - (now().getTime() % 1000));
  };
  if (texts.length > 0 || faces.length > 0) schedule();

  // CSS animations are throttled while hidden; re-seed the offsets on return
  // so the hands never come back showing a stale time.
  const onVisible = () => {
    if (document.visibilityState !== "visible") return;
    const date = now();
    for (const face of faces) seedHands(face, date);
    tick();
  };
  document.addEventListener("visibilitychange", onVisible);

  return () => {
    if (timer !== undefined) clearTimeout(timer);
    document.removeEventListener("visibilitychange", onVisible);
  };
}

/** `null` where the environment has no media queries (unit tests). */
function reducedMotionQuery(): MediaQueryList | null {
  return typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-reduced-motion: reduce)")
    : null;
}

function readFormat(raw: string | null): ClockFormat {
  const formats: ClockFormat[] = [
    "time",
    "date",
    "weekday",
    "week",
    "offset",
    "unix",
  ];
  return formats.find((format) => format === raw) ?? "time";
}

/** Builds the face once: dial, ticks, three hands. Idempotent per element. */
function renderFace(el: Element, date: Date): void {
  const doc = el.ownerDocument;
  const svg = doc.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 100 100");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("class", "sb-clock-face");

  const dial = doc.createElementNS(SVG_NS, "circle");
  dial.setAttribute("cx", "50");
  dial.setAttribute("cy", "50");
  dial.setAttribute("r", "47");
  dial.setAttribute("class", "sb-clock-dial");
  svg.appendChild(dial);

  for (let hour = 0; hour < 12; hour++) {
    const mark = doc.createElementNS(SVG_NS, "line");
    const long = hour % 3 === 0;
    mark.setAttribute("x1", "50");
    mark.setAttribute("y1", long ? "8" : "10");
    mark.setAttribute("x2", "50");
    mark.setAttribute("y2", long ? "17" : "15");
    mark.setAttribute("transform", `rotate(${String(hour * 30)} 50 50)`);
    mark.setAttribute(
      "class",
      long ? "sb-clock-tick sb-active" : "sb-clock-tick",
    );
    svg.appendChild(mark);
  }

  for (const [name, y] of [
    ["hour", "26"],
    ["minute", "14"],
    ["second", "12"],
  ] as const) {
    const hand = doc.createElementNS(SVG_NS, "line");
    hand.setAttribute("x1", "50");
    hand.setAttribute("y1", "54");
    hand.setAttribute("x2", "50");
    hand.setAttribute("y2", y);
    hand.setAttribute("class", `sb-clock-hand sb-clock-${name}`);
    svg.appendChild(hand);
  }

  const cap = doc.createElementNS(SVG_NS, "circle");
  cap.setAttribute("cx", "50");
  cap.setAttribute("cy", "50");
  cap.setAttribute("r", "3.5");
  cap.setAttribute("class", "sb-clock-cap");
  svg.appendChild(cap);

  el.replaceChildren(svg);
  seedHands(el, date);
}

/**
 * Points the hands at `date` by giving each animation a negative delay equal
 * to how far into its period the current time already is.
 */
export function seedHands(el: Element, date: Date): void {
  const zone = el.getAttribute("data-clock") ?? "";
  const since = secondsSinceMidnight(date, zone);
  const delays: [string, number][] = [
    ["sb-clock-hour", HOUR_PERIOD],
    ["sb-clock-minute", MINUTE_PERIOD],
    ["sb-clock-second", SECOND_PERIOD],
  ];
  for (const [className, period] of delays) {
    const hand = el.querySelector(`.${className}`);
    if (hand instanceof SVGElement) {
      hand.style.animationDuration = `${String(period)}s`;
      hand.style.animationDelay = `-${String(since % period)}s`;
    }
  }
}

/** Intl parts as a plain lookup; an unknown zone falls back to the system. */
function partsIn(
  date: Date,
  timeZone: string,
  options: Intl.DateTimeFormatOptions,
): Record<string, string> {
  const config: Intl.DateTimeFormatOptions =
    timeZone === "" ? options : { ...options, timeZone };
  let parts: Intl.DateTimeFormatPart[];
  try {
    parts = formatter("en-GB", config).formatToParts(date);
  } catch {
    parts = formatter("en-GB", options).formatToParts(date);
  }
  return Object.fromEntries(parts.map((part) => [part.type, part.value]));
}

function formatter(
  locale: string,
  options: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormat {
  // The default zone is captured when Intl constructs the formatter. Do not
  // cache it: a running clock must follow an OS timezone change immediately.
  if (options.timeZone === undefined) {
    return new Intl.DateTimeFormat(locale, options);
  }
  const key = `${locale}\u0000${JSON.stringify(options)}`;
  const cached = formatters.get(key);
  if (cached !== undefined) return cached;
  const created = new Intl.DateTimeFormat(locale, options);
  if (formatters.size >= FORMATTER_CACHE_LIMIT) formatters.clear();
  formatters.set(key, created);
  return created;
}
