/**
 * Sortable and filterable tables.
 *
 * Both are the APG patterns: a real `<button>` inside the `<th>` carries the
 * sort (keyboard support comes free), and filtered-out rows get the `hidden`
 * attribute, which removes them from layout AND the accessibility tree.
 */
import { t } from "../../i18n/t";

import { behaviour, onSync, queryAll } from "./delegate";

/** Debounce for the result count so a screen reader is not cut off mid-word. */
const COUNT_MS = 400;

const countTimers = new WeakMap<HTMLElement, number>();

/** The direct row children of a section — `.rows` is a legacy collection
 * that is not implemented everywhere the shell runs. */
function rowsOf(section: Element): HTMLTableRowElement[] {
  return Array.from(
    section.querySelectorAll<HTMLTableRowElement>(":scope > tr"),
  );
}

/** The cells of one row, in order. */
function cellsOf(row: Element): HTMLElement[] {
  return Array.from(
    row.querySelectorAll<HTMLElement>(":scope > th, :scope > td"),
  );
}

/** A cell's comparable text. */
function cellText(row: HTMLTableRowElement, index: number): string {
  return (cellsOf(row)[index]?.textContent ?? "").trim();
}

/**
 * A numeric cell's value. Currency symbols and thousands separators are
 * stripped; anything left unparsable sorts to the start.
 */
function cellNumber(text: string): number {
  const value = parseFloat(text.replace(/[^0-9.+-]/g, ""));
  return Number.isNaN(value) ? -Infinity : value;
}

const collator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

behaviour("click", "th[data-sb-sort]", (header) => {
  const body = header.closest("table")?.querySelector("tbody") ?? null;
  const headerRow = header.closest("tr");
  if (body === null || headerRow === null) return;

  const ascending = header.getAttribute("aria-sort") !== "ascending";
  // aria-sort marks exactly one column, so clear the rest of the row first.
  for (const other of queryAll(headerRow, "th[aria-sort]")) {
    other.removeAttribute("aria-sort");
  }
  header.setAttribute("aria-sort", ascending ? "ascending" : "descending");

  const index = cellsOf(headerRow).indexOf(header);
  const numeric = header.dataset.sbSort === "number";
  const direction = ascending ? 1 : -1;
  const rows = rowsOf(body).sort((a, b) => {
    const left = cellText(a, index);
    const right = cellText(b, index);
    const order = numeric
      ? cellNumber(left) - cellNumber(right)
      : collator.compare(left, right);
    return order * direction;
  });
  body.append(...rows);
});

/** The table a filter input points at, looked up inside the plugin's tree. */
function filterTarget(input: HTMLElement): HTMLTableElement | null {
  const selector = input.dataset.sbFilter;
  if (selector === undefined || selector === "") return null;
  const root = input.getRootNode();
  if (!(root instanceof ShadowRoot || root instanceof Document)) return null;
  try {
    const found = root.querySelector(selector);
    return found instanceof HTMLTableElement ? found : null;
  } catch {
    return null; // An invalid selector is a plugin bug, not a crash.
  }
}

/** Marks a counter as a polite live region unless it declares its own. */
function ensureLive(counter: HTMLElement): void {
  if (!counter.hasAttribute("role") && !counter.hasAttribute("aria-live")) {
    counter.setAttribute("role", "status");
  }
}

/**
 * Applies one filter input to its table.
 *
 * `announceCount` is false on a re-render sync: the rows must be re-hidden
 * immediately, but the count has not changed for the user and re-announcing
 * it every second would make the plugin unusable with a screen reader.
 */
function applyFilter(input: HTMLElement, announceCount: boolean): void {
  if (!(input instanceof HTMLInputElement)) return;
  const table = filterTarget(input);
  const body = table?.querySelector("tbody") ?? null;
  if (body === null) return;

  const rows = rowsOf(body);
  const query = input.value.trim().toLowerCase();
  let visible = 0;
  for (const row of rows) {
    const match = query === "" || row.textContent.toLowerCase().includes(query);
    row.hidden = !match;
    if (match) visible += 1;
  }

  const scope =
    table?.closest(".sb-table-wrap") ?? table?.parentElement ?? null;
  const empty = scope?.querySelector<HTMLElement>(".sb-table-filter__empty");
  if (empty != null) empty.hidden = !(query !== "" && visible === 0);

  const selector = input.dataset.sbFilter;
  const root = input.getRootNode();
  if (!(root instanceof ShadowRoot || root instanceof Document)) return;
  const counter = queryAll(root, "[data-sb-filter-count]").find(
    (element) => element.dataset.sbFilterCount === selector,
  );
  if (counter === undefined) return;
  ensureLive(counter);
  const text = t("kit.filterCount")
    .replace("{shown}", String(visible))
    .replace("{total}", String(rows.length));
  if (!announceCount) {
    counter.textContent = text;
    return;
  }
  clearTimeout(countTimers.get(counter));
  countTimers.set(
    counter,
    window.setTimeout(() => {
      countTimers.delete(counter);
      counter.textContent = text;
    }, COUNT_MS),
  );
}

behaviour("input", "[data-sb-filter]", (input) => {
  applyFilter(input, true);
});

// A re-render brings every row back visible. The typed query survives in the
// input itself (PluginContent restores `input[data-field]` values), so give a
// filter input a data-field and the filter re-applies itself here.
onSync((root) => {
  for (const input of queryAll(root, "[data-sb-filter]")) {
    applyFilter(input, false);
  }
});
