/**
 * Combobox — a themed suggestion list on a text input (APG editable combobox
 * with list autocomplete).
 *
 * A plugin writes an `<input list="…">` with a `<datalist>`, which already
 * works on its own. This replaces the native popup — unstylable and drawn by
 * the OS, so in a dock window it appears detached from the bar — with a list
 * built from the same options. Free text stays allowed.
 */
import { behaviour, onSync, queryAll } from "./delegate";

let listSequence = 0;

/** The input and datalist of one combobox field. */
function partsOf(root: HTMLElement): {
  input: HTMLInputElement | null;
  datalist: HTMLDataListElement | null;
} {
  const input =
    root.querySelector<HTMLInputElement>("input[list]") ??
    root.querySelector<HTMLInputElement>("input");
  const listId = input?.getAttribute("list");
  const scope = root.getRootNode();
  let datalist: HTMLDataListElement | null = null;
  if (
    listId != null &&
    (scope instanceof ShadowRoot || scope instanceof Document)
  ) {
    // Scoped to the plugin's own root: IDs are shadow-scoped, which is what
    // makes `list=` safe to allow in the first place.
    const found = scope.getElementById(listId);
    datalist = found instanceof HTMLDataListElement ? found : null;
  }
  return { input, datalist: datalist ?? root.querySelector("datalist") };
}

/**
 * The themed listbox for a field, created on first use.
 *
 * The native `list` attribute is removed only once the replacement exists,
 * so a field is never left without either.
 */
function ensureList(root: HTMLElement): HTMLElement | null {
  const existing = root.querySelector<HTMLElement>(".sb-combobox__list");
  if (existing !== null) return existing;
  const { input, datalist } = partsOf(root);
  if (input === null) return null;
  const list = document.createElement("ul");
  list.className = "sb-combobox__list";
  listSequence += 1;
  list.id = `sb-combobox-list-${String(listSequence)}`;
  list.setAttribute("role", "listbox");
  root.append(list);
  input.removeAttribute("list");
  input.setAttribute("role", "combobox");
  input.setAttribute("aria-expanded", "false");
  input.setAttribute("aria-autocomplete", "list");
  input.setAttribute("aria-controls", list.id);
  input.autocomplete = "off";
  list.dataset.sbSource = datalist?.id ?? "";
  return list;
}

/** The option values a list draws from. */
function optionValues(root: HTMLElement, list: HTMLElement): string[] {
  const scope = root.getRootNode();
  const sourceId = list.dataset.sbSource ?? "";
  if (sourceId === "") return [];
  if (!(scope instanceof ShadowRoot || scope instanceof Document)) return [];
  const datalist = scope.getElementById(sourceId);
  if (!(datalist instanceof HTMLDataListElement)) return [];
  // `.options` is a legacy collection; querying the elements works the same
  // and does not depend on it being implemented.
  return Array.from(datalist.querySelectorAll("option"), (option) =>
    option.value === "" ? option.textContent.trim() : option.value,
  );
}

/** Rebuilds the visible options for a query. Returns how many matched. */
function renderOptions(root: HTMLElement, query: string): number {
  const list = ensureList(root);
  if (list === null) return 0;
  const needle = query.trim().toLowerCase();
  const matches = optionValues(root, list).filter(
    (value) => needle === "" || value.toLowerCase().includes(needle),
  );
  list.textContent = "";
  matches.forEach((value, index) => {
    const option = document.createElement("li");
    option.className = "sb-combobox__option";
    option.id = `${list.id}-${String(index)}`;
    option.setAttribute("role", "option");
    option.textContent = value;
    list.append(option);
  });
  return matches.length;
}

/** Marks one option active. Focus stays in the input (APG). */
function setActive(root: HTMLElement, item: HTMLElement | null): void {
  const list = root.querySelector<HTMLElement>(".sb-combobox__list");
  const { input } = partsOf(root);
  if (list === null || input === null) return;
  for (const previous of queryAll(list, ".is-active")) {
    previous.classList.remove("is-active");
  }
  if (item === null) {
    input.removeAttribute("aria-activedescendant");
    for (const option of Array.from(list.children)) {
      option.setAttribute("aria-selected", "false");
    }
    return;
  }
  item.classList.add("is-active");
  item.scrollIntoView({ block: "nearest" });
  input.setAttribute("aria-activedescendant", item.id);
  for (const option of Array.from(list.children)) {
    option.setAttribute("aria-selected", String(option === item));
  }
}

function closeCombobox(root: HTMLElement): void {
  const list = root.querySelector(".sb-combobox__list");
  if (list === null) return;
  list.classList.remove("is-open");
  setActive(root, null);
  partsOf(root).input?.setAttribute("aria-expanded", "false");
}

function openCombobox(root: HTMLElement, query: string): void {
  if (renderOptions(root, query) === 0) {
    closeCombobox(root);
    return;
  }
  root.querySelector(".sb-combobox__list")?.classList.add("is-open");
  partsOf(root).input?.setAttribute("aria-expanded", "true");
}

function isOpen(root: HTMLElement): boolean {
  return root.querySelector(".sb-combobox__list.is-open") !== null;
}

/** True while commit() is dispatching its own input event. */
let committing = false;

/**
 * Writes a chosen value and closes.
 *
 * The input event has to reach the plugin like any other field change, but it
 * would also reach this module's own input handler and re-open the list that
 * was just dismissed. The flag suppresses exactly that one re-entry —
 * `isTrusted` would do it too, but it would equally reject a value a plugin
 * sets programmatically.
 */
function commit(root: HTMLElement, value: string): void {
  const { input } = partsOf(root);
  if (input === null) return;
  input.value = value;
  committing = true;
  try {
    input.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
    input.dispatchEvent(new Event("change", { bubbles: true, composed: true }));
  } finally {
    committing = false;
  }
  closeCombobox(root);
  input.focus();
}

behaviour("input", "[data-sb-combobox]", (root) => {
  if (committing) return;
  const { input } = partsOf(root);
  if (input !== null) openCombobox(root, input.value);
});

behaviour("click", "*", (element) => {
  const root = element.closest<HTMLElement>("[data-sb-combobox]");
  if (root === null) {
    const scope = element.getRootNode();
    if (scope instanceof ShadowRoot || scope instanceof Document) {
      for (const other of queryAll(scope, "[data-sb-combobox]")) {
        closeCombobox(other);
      }
    }
    return;
  }
  const option = element.closest(".sb-combobox__option");
  if (option !== null) {
    commit(root, option.textContent);
    return;
  }
  if (element.matches("input")) {
    openCombobox(
      root,
      element instanceof HTMLInputElement ? element.value : "",
    );
  }
});

// mousedown on an option must not blur the input before the click lands.
behaviour("mousedown", ".sb-combobox__option", (_option, event) => {
  event.preventDefault();
});

behaviour("focusout", "[data-sb-combobox]", (root, event) => {
  const next = event instanceof FocusEvent ? event.relatedTarget : null;
  if (!(next instanceof Node) || !root.contains(next)) closeCombobox(root);
});

behaviour("keydown", "[data-sb-combobox]", (root, event) => {
  if (!(event instanceof KeyboardEvent)) return;
  const from = event.composedPath()[0];
  if (!(from instanceof HTMLElement) || !from.matches("input")) return;
  const list = root.querySelector<HTMLElement>(".sb-combobox__list");

  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    if (!isOpen(root)) {
      openCombobox(root, from instanceof HTMLInputElement ? from.value : "");
    }
    if (!isOpen(root) || list === null) return;
    const items = Array.from(list.children).filter(
      (child): child is HTMLElement => child instanceof HTMLElement,
    );
    const active = list.querySelector<HTMLElement>(".is-active");
    const current = active === null ? -1 : items.indexOf(active);
    const step = event.key === "ArrowDown" ? 1 : -1;
    const at = (current + step + items.length) % items.length;
    setActive(root, items[at] ?? null);
    return;
  }
  if (event.key === "Enter") {
    const active = isOpen(root)
      ? (list?.querySelector<HTMLElement>(".is-active") ?? null)
      : null;
    if (active !== null) {
      event.preventDefault();
      commit(root, active.textContent);
    }
    return;
  }
  if (event.key === "Escape" && isOpen(root)) {
    // The innermost layer wins: this Escape must not also close the flyout.
    event.stopPropagation();
    closeCombobox(root);
  }
});

// A re-render replaces the markup, so the themed list has to be re-attached
// and the native popup suppressed again.
onSync((root) => {
  for (const field of queryAll(root, "[data-sb-combobox]")) ensureList(field);
});
