/**
 * Date picker and date-range picker (APG date-picker-dialog).
 *
 * `<input type="date">` is allowed and works — this is for the two things it
 * cannot do: render in the bar's own theme rather than an OS popup that
 * escapes a dock window, and pick a RANGE with the span marked between the
 * endpoints.
 *
 * Both pickers share one calendar: the only difference is which days are
 * marked and what a click commits, so `RangeMarks` carries that difference
 * instead of a second implementation.
 */
import { safeIntlLocale, t } from "../../i18n/t";

import { behaviour, queryAll } from "./delegate";

/** Any Monday — used to label the weekday header without hardcoding names. */
const A_MONDAY = new Date(2024, 0, 1);

interface CalendarFormat {
  readonly monthTitle: Intl.DateTimeFormat;
  readonly dayLabel: Intl.DateTimeFormat;
  readonly fieldLabel: Intl.DateTimeFormat;
  readonly weekdayShort: Intl.DateTimeFormat;
  readonly weekdayLong: Intl.DateTimeFormat;
}

const formatCache = new Map<string, CalendarFormat>();

/**
 * Intl formatters for the configured language.
 *
 * `document.documentElement.lang` is where `bridge.ts` puts the resolved app
 * language, so the calendar follows the setting without threading it through
 * every call. Formatters are cached — constructing them is expensive.
 */
function calendarFormat(): CalendarFormat {
  const locale = safeIntlLocale(document.documentElement.lang || "en");
  const cached = formatCache.get(locale);
  if (cached !== undefined) return cached;
  const format: CalendarFormat = {
    monthTitle: new Intl.DateTimeFormat(locale, {
      month: "long",
      year: "numeric",
    }),
    dayLabel: new Intl.DateTimeFormat(locale, { dateStyle: "full" }),
    fieldLabel: new Intl.DateTimeFormat(locale, {
      day: "numeric",
      month: "short",
      year: "numeric",
    }),
    weekdayShort: new Intl.DateTimeFormat(locale, { weekday: "short" }),
    weekdayLong: new Intl.DateTimeFormat(locale, { weekday: "long" }),
  };
  formatCache.set(locale, format);
  return format;
}

/** The compact date label shared by themed input controls. */
export function formatDate(date: Date): string {
  return calendarFormat().fieldLabel.format(date);
}

/** A date as `YYYY-MM-DD` in local time. */
export function toISO(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${String(date.getFullYear())}-${month}-${day}`;
}

/** Parses `YYYY-MM-DD`, rejecting dates the calendar would roll over. */
function fromISO(value: string | undefined): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value ?? "");
  if (match === null) return null;
  const [, year, month, day] = match;
  const parsed = new Date(Number(year), Number(month) - 1, Number(day));
  return parsed.getMonth() === Number(month) - 1 ? parsed : null;
}

function addDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

/** Adds months, clamping to the last valid day (Jan 31 + 1 month = Feb 28). */
function addMonths(date: Date, months: number): Date {
  const lastDay = new Date(
    date.getFullYear(),
    date.getMonth() + months + 1,
    0,
  ).getDate();
  return new Date(
    date.getFullYear(),
    date.getMonth() + months,
    Math.min(date.getDate(), lastDay),
  );
}

/** Which days the calendar marks: one date, or a span with its endpoints. */
interface RangeMarks {
  readonly start: string;
  readonly end: string;
}

function navButton(direction: string, label: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `sb-datepicker__nav sb-datepicker__nav--${direction}`;
  button.setAttribute("aria-label", label);
  return button;
}

/** The calendar panel for a field, built once and reused. */
function ensurePanel(wrapper: HTMLElement, label: string): HTMLElement {
  const existing = wrapper.querySelector<HTMLElement>(".sb-datepicker");
  if (existing !== null && existing.dataset.sbReady === "true") return existing;
  const panel = existing ?? document.createElement("div");
  if (existing === null) wrapper.append(panel);
  panel.className = "sb-datepicker sb-datepicker--anchored";
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", label);
  panel.dataset.sbReady = "true";

  const format = calendarFormat();
  const header = document.createElement("div");
  header.className = "sb-datepicker__header";
  const title = document.createElement("div");
  title.className = "sb-datepicker__title";
  title.setAttribute("aria-live", "polite");
  header.append(
    navButton("prev", t("kit.previousMonth")),
    title,
    navButton("next", t("kit.nextMonth")),
  );

  const grid = document.createElement("div");
  grid.className = "sb-datepicker__grid";
  grid.setAttribute("role", "grid");
  const weekdays = document.createElement("div");
  weekdays.className = "sb-datepicker__weekdays";
  weekdays.setAttribute("role", "row");
  for (let index = 0; index < 7; index += 1) {
    const day = addDays(A_MONDAY, index);
    const cell = document.createElement("span");
    cell.className = "sb-datepicker__weekday";
    cell.setAttribute("role", "columnheader");
    cell.setAttribute("aria-label", format.weekdayLong.format(day));
    cell.textContent = format.weekdayShort.format(day);
    weekdays.append(cell);
  }
  grid.append(weekdays);
  panel.replaceChildren(header, grid);
  return panel;
}

/** Renders the month around `active`, marking the selected day or span. */
function renderMonth(
  panel: HTMLElement,
  active: Date,
  marks: RangeMarks,
): void {
  panel.dataset.sbActive = toISO(active);
  const format = calendarFormat();
  const title = format.monthTitle.format(active);
  const heading = panel.querySelector(".sb-datepicker__title");
  if (heading !== null) heading.textContent = title;
  const grid = panel.querySelector(".sb-datepicker__grid");
  if (grid === null) return;
  grid.setAttribute("aria-label", title);
  for (const row of queryAll(grid, ".sb-datepicker__week")) row.remove();

  const todayISO = toISO(new Date());
  const span =
    marks.start !== "" && marks.end !== "" && marks.start !== marks.end;
  const first = new Date(active.getFullYear(), active.getMonth(), 1);
  // Weeks start Monday: getDay() is 0 for Sunday, so shift by 6.
  let cursor = addDays(first, -((first.getDay() + 6) % 7));
  for (let week = 0; week < 6; week += 1) {
    const row = document.createElement("div");
    row.className = "sb-datepicker__week";
    row.setAttribute("role", "row");
    for (let index = 0; index < 7; index += 1) {
      const iso = toISO(cursor);
      const classes = ["sb-datepicker__day"];
      if (cursor.getMonth() !== active.getMonth()) {
        classes.push("sb-datepicker__day--outside");
      }
      if (span) {
        if (iso > marks.start && iso < marks.end) classes.push("is-in-range");
        if (iso === marks.start) classes.push("is-range-start");
        if (iso === marks.end) classes.push("is-range-end");
      }
      const day = document.createElement("button");
      day.type = "button";
      day.className = classes.join(" ");
      day.setAttribute("role", "gridcell");
      day.dataset.date = iso;
      day.tabIndex = iso === panel.dataset.sbActive ? 0 : -1;
      day.setAttribute(
        "aria-selected",
        String(iso === marks.start || iso === marks.end),
      );
      day.setAttribute("aria-label", format.dayLabel.format(cursor));
      if (iso === todayISO) day.setAttribute("aria-current", "date");
      day.textContent = String(cursor.getDate());
      row.append(day);
      cursor = addDays(cursor, 1);
    }
    grid.append(row);
  }
}

/** How a picker variant reads and writes its field. */
interface PickerKind {
  readonly attribute: string;
  readonly label: string;
  /** The days to mark, given the panel's pending first pick. */
  marks: (wrapper: HTMLElement, panel: HTMLElement) => RangeMarks;
  /** Commits a clicked day. Returns false while the picker stays open. */
  commit: (wrapper: HTMLElement, panel: HTMLElement, iso: string) => boolean;
}

function inputsOf(wrapper: HTMLElement): HTMLInputElement[] {
  return Array.from(wrapper.querySelectorAll<HTMLInputElement>("input"));
}

/** Writes a value and lets the plugin's form contract see it. */
function setValue(input: HTMLInputElement | undefined, iso: string): void {
  if (input === undefined) return;
  input.value = iso;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

const SINGLE: PickerKind = {
  attribute: "data-sb-datepicker",
  label: "kit.chooseDate",
  marks: (wrapper) => {
    const value = inputsOf(wrapper)[0]?.value ?? "";
    const iso = fromISO(value) === null ? "" : value;
    return { start: iso, end: iso };
  },
  commit: (wrapper, _panel, iso) => {
    setValue(inputsOf(wrapper)[0], iso);
    return true;
  },
};

const RANGE: PickerKind = {
  attribute: "data-sb-daterange",
  label: "kit.chooseDateRange",
  marks: (wrapper, panel) => {
    const pending = panel.dataset.sbPending ?? "";
    const inputs = inputsOf(wrapper);
    const stored = (index: number) => {
      const value = inputs[index]?.value ?? "";
      return fromISO(value) === null ? "" : value;
    };
    let start = pending === "" ? stored(0) : pending;
    let end = pending === "" ? stored(1) : "";
    if (start !== "" && end !== "" && end < start) [start, end] = [end, start];
    return { start, end };
  },
  commit: (wrapper, panel, iso) => {
    if (panel.dataset.sbPending === undefined) {
      panel.dataset.sbPending = iso; // First pick is the range start.
      return false;
    }
    let start = panel.dataset.sbPending;
    let end = iso;
    if (end < start) [start, end] = [end, start];
    const inputs = inputsOf(wrapper);
    setValue(inputs[0], start);
    setValue(inputs[1], end);
    return true;
  },
};

function panelOf(wrapper: HTMLElement, kind: PickerKind): HTMLElement {
  return ensurePanel(wrapper, t(kind.label));
}

function draw(wrapper: HTMLElement, kind: PickerKind, active: Date): void {
  const panel = panelOf(wrapper, kind);
  renderMonth(panel, active, kind.marks(wrapper, panel));
}

function openPicker(
  wrapper: HTMLElement,
  kind: PickerKind,
  source: HTMLInputElement | null,
): void {
  if (wrapper.classList.contains("is-open")) return;
  const panel = panelOf(wrapper, kind);
  delete panel.dataset.sbPending;
  const inputs = inputsOf(wrapper);
  panel.dataset.sbOpener = String(
    Math.max(source === null ? 0 : inputs.indexOf(source), 0),
  );
  const seed =
    fromISO(source?.value) ?? fromISO(inputs[0]?.value) ?? new Date();
  draw(wrapper, kind, seed);
  wrapper.classList.add("is-open");
  wrapper
    .querySelector("[data-sb-datepicker-toggle]")
    ?.setAttribute("aria-expanded", "true");
}

function closePicker(
  wrapper: HTMLElement,
  kind: PickerKind,
  refocus: boolean,
): void {
  const panel = panelOf(wrapper, kind);
  delete panel.dataset.sbPending;
  wrapper.classList.remove("is-open");
  const toggle = wrapper.querySelector<HTMLElement>(
    "[data-sb-datepicker-toggle]",
  );
  toggle?.setAttribute("aria-expanded", "false");
  if (!refocus) return;
  const opener =
    toggle ?? inputsOf(wrapper)[Number(panel.dataset.sbOpener ?? 0)];
  opener?.focus();
}

/** One day-grid keystroke, or null when the key is not a calendar move. */
function keyMove(active: Date, key: string): Date | null {
  const weekday = (active.getDay() + 6) % 7;
  switch (key) {
    case "ArrowLeft":
      return addDays(active, -1);
    case "ArrowRight":
      return addDays(active, 1);
    case "ArrowUp":
      return addDays(active, -7);
    case "ArrowDown":
      return addDays(active, 7);
    case "Home":
      return addDays(active, -weekday);
    case "End":
      return addDays(active, 6 - weekday);
    case "PageUp":
      return addMonths(active, -1);
    case "PageDown":
      return addMonths(active, 1);
    default:
      return null;
  }
}

/** Registers both pickers; they differ only in their {@link PickerKind}. */
function install(kind: PickerKind): void {
  const root = `[${kind.attribute}]`;

  behaviour("click", "*", (element) => {
    const wrapper = element.closest<HTMLElement>(root);
    const scope = element.getRootNode();
    if (scope instanceof ShadowRoot || scope instanceof Document) {
      // Light dismiss: any click closes every other open picker.
      for (const other of queryAll(scope, `${root}.is-open`)) {
        if (other !== wrapper) closePicker(other, kind, false);
      }
    }
    if (wrapper === null) return;

    if (element.closest("[data-sb-datepicker-toggle]") !== null) {
      if (wrapper.classList.contains("is-open")) {
        closePicker(wrapper, kind, false);
      } else {
        openPicker(wrapper, kind, null);
      }
      return;
    }
    const input = element.closest("input");
    if (input !== null) {
      openPicker(wrapper, kind, input);
      return;
    }
    const nav = element.closest(".sb-datepicker__nav");
    if (nav !== null) {
      const panel = panelOf(wrapper, kind);
      const active = fromISO(panel.dataset.sbActive) ?? new Date();
      const step = nav.classList.contains("sb-datepicker__nav--prev") ? -1 : 1;
      draw(wrapper, kind, addMonths(active, step));
      return;
    }
    const day = element.closest<HTMLElement>(".sb-datepicker__day");
    const iso = day?.dataset.date;
    if (iso === undefined) return;
    const panel = panelOf(wrapper, kind);
    if (kind.commit(wrapper, panel, iso)) {
      closePicker(wrapper, kind, true);
      return;
    }
    // Still picking: redraw with the pending start marked and stay put.
    draw(wrapper, kind, fromISO(iso) ?? new Date());
    panel.querySelector<HTMLElement>(`[data-date="${iso}"]`)?.focus();
  });

  behaviour("keydown", root, (wrapper, event) => {
    if (!(event instanceof KeyboardEvent)) return;
    const from = event.composedPath()[0];
    if (!(from instanceof HTMLElement)) return;

    if (event.key === "Escape") {
      if (wrapper.classList.contains("is-open")) {
        event.preventDefault();
        event.stopPropagation();
        closePicker(wrapper, kind, true);
      }
      return;
    }
    if (
      from.matches("input, [data-sb-datepicker-toggle]") &&
      event.key === "ArrowDown"
    ) {
      event.preventDefault();
      openPicker(wrapper, kind, from instanceof HTMLInputElement ? from : null);
      panelOf(wrapper, kind)
        .querySelector<HTMLElement>('.sb-datepicker__day[tabindex="0"]')
        ?.focus();
      return;
    }
    const day = from.closest<HTMLElement>(".sb-datepicker__day");
    const active = fromISO(day?.dataset.date);
    if (active === null) return;
    const next = keyMove(active, event.key);
    if (next === null) return;
    event.preventDefault();
    draw(wrapper, kind, next);
    panelOf(wrapper, kind)
      .querySelector<HTMLElement>(`[data-date="${toISO(next)}"]`)
      ?.focus();
  });

  // Tabbing out of an open picker closes it, matching the other overlays.
  behaviour("focusout", root, (wrapper, event) => {
    const next = event instanceof FocusEvent ? event.relatedTarget : null;
    if (next instanceof Element && !wrapper.contains(next)) {
      closePicker(wrapper, kind, false);
    }
  });
}

install(SINGLE);
install(RANGE);
