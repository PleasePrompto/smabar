/** Themed select popup backed by the plugin's real `<select>` element. */
import { behaviour, onSync, queryAll } from "./delegate";
import { buildSelectOptions } from "./selectOptions";

let selectSequence = 0;
let openShell: HTMLElement | null = null;
const resetForms = new WeakSet<HTMLFormElement>();
const SELECT_VIEWPORT_GAP_PX = 4;
const TYPEAHEAD_RESET_MS = 500;
const NATIVE_INVALID_ATTR = "data-sb-native-invalid";

function sourceOf(shell: ParentNode): HTMLSelectElement | null {
  const source = shell.querySelector("select.sb-select-source");
  return source instanceof HTMLSelectElement ? source : null;
}

function triggerOf(shell: ParentNode): HTMLButtonElement | null {
  const trigger = shell.querySelector(".sb-select-trigger");
  return trigger instanceof HTMLButtonElement ? trigger : null;
}

function listOf(shell: ParentNode): HTMLElement | null {
  return shell.querySelector(".sb-select-list");
}

function optionText(option: HTMLOptionElement): string {
  return (
    option.label || option.text || option.textContent.trim() || option.value
  );
}

function optionsOf(shell: ParentNode): HTMLElement[] {
  return queryAll(shell, ".sb-select-option").filter(
    (option) => option.getAttribute("aria-disabled") !== "true",
  );
}

function activeOption(shell: ParentNode): HTMLElement | null {
  return shell.querySelector<HTMLElement>(".sb-select-option[data-active]");
}

function setActive(shell: HTMLElement, item: HTMLElement | null): void {
  for (const option of queryAll(shell, ".sb-select-option[data-active]")) {
    option.removeAttribute("data-active");
  }
  const trigger = triggerOf(shell);
  if (item === null) {
    trigger?.removeAttribute("aria-activedescendant");
    return;
  }
  item.toggleAttribute("data-active", true);
  trigger?.setAttribute("aria-activedescendant", item.id);
  if (typeof item.scrollIntoView === "function") {
    item.scrollIntoView({ block: "nearest" });
  }
}

function popoverOpen(list: HTMLElement): boolean {
  try {
    return list.matches(":popover-open");
  } catch {
    return false;
  }
}

function positionTopLayer(trigger: HTMLElement, list: HTMLElement): void {
  const rect = trigger.getBoundingClientRect();
  const viewportWidth = window.innerWidth;
  const viewportHeight = window.innerHeight;
  const below = viewportHeight - rect.bottom - SELECT_VIEWPORT_GAP_PX;
  const above = rect.top - SELECT_VIEWPORT_GAP_PX;
  const configuredMax = Number.parseFloat(getComputedStyle(list).maxHeight);
  const desiredHeight = Math.min(
    list.scrollHeight,
    Number.isFinite(configuredMax) ? configuredMax : list.scrollHeight,
  );
  const opensUp = above > below && desiredHeight > below;
  const available = Math.max(
    0,
    (opensUp ? above : below) - SELECT_VIEWPORT_GAP_PX,
  );
  const width = Math.max(
    0,
    Math.min(rect.width, viewportWidth - 2 * SELECT_VIEWPORT_GAP_PX),
  );
  const left = Math.min(
    Math.max(SELECT_VIEWPORT_GAP_PX, rect.left),
    Math.max(
      SELECT_VIEWPORT_GAP_PX,
      viewportWidth - width - SELECT_VIEWPORT_GAP_PX,
    ),
  );
  list.style.position = "fixed";
  list.style.inset = "auto";
  list.style.left = `${String(left)}px`;
  list.style.width = `${String(width)}px`;
  list.style.maxHeight = `${String(Math.min(available, desiredHeight))}px`;
  if (opensUp) {
    list.style.bottom = `${String(viewportHeight - rect.top + SELECT_VIEWPORT_GAP_PX)}px`;
    list.style.top = "auto";
  } else {
    list.style.top = `${String(rect.bottom + SELECT_VIEWPORT_GAP_PX)}px`;
    list.style.bottom = "auto";
  }
}

function showList(trigger: HTMLElement, list: HTMLElement): void {
  if (typeof list.showPopover !== "function") return;
  try {
    if (!popoverOpen(list)) list.showPopover();
    list.toggleAttribute("data-top-layer", true);
    positionTopLayer(trigger, list);
  } catch {
    list.removeAttribute("data-top-layer");
    list.removeAttribute("style");
  }
}

function hideList(list: HTMLElement): void {
  if (typeof list.hidePopover === "function" && popoverOpen(list)) {
    list.hidePopover();
  }
  list.removeAttribute("data-top-layer");
  list.removeAttribute("style");
}

function setOpen(shell: HTMLElement, open: boolean): void {
  const trigger = triggerOf(shell);
  const list = listOf(shell);
  shell.toggleAttribute("data-open", open);
  list?.toggleAttribute("data-open", open);
  if (list !== null) list.hidden = !open;
  trigger?.setAttribute("aria-expanded", String(open));
  if (open && trigger !== null && list !== null) showList(trigger, list);
  else if (list !== null) hideList(list);
}

function close(shell: HTMLElement, restoreFocus: boolean): void {
  setOpen(shell, false);
  setActive(shell, null);
  if (openShell === shell) openShell = null;
  if (restoreFocus) triggerOf(shell)?.focus();
}

function activateOption(shell: HTMLElement, edge?: "first" | "last"): void {
  const options = optionsOf(shell);
  const selected = options.find(
    (option) => option.getAttribute("aria-selected") === "true",
  );
  const target =
    edge === "first"
      ? options[0]
      : edge === "last"
        ? options[options.length - 1]
        : (selected ?? options[0]);
  setActive(shell, target ?? null);
}

function open(shell: HTMLElement, edge?: "first" | "last"): void {
  const source = sourceOf(shell);
  if (source === null || source.disabled) return;
  if (openShell !== null && openShell !== shell) close(openShell, false);
  openShell = shell;
  setOpen(shell, true);
  activateOption(shell, edge);
  triggerOf(shell)?.focus();
}

function moveActive(shell: HTMLElement, step: number): void {
  const options = optionsOf(shell);
  if (options.length === 0) return;
  const current = activeOption(shell);
  const index = current === null ? -1 : options.indexOf(current);
  const next =
    index === -1 ? (step > 0 ? 0 : options.length - 1) : index + step;
  const wrapped = ((next % options.length) + options.length) % options.length;
  setActive(shell, options[wrapped] ?? null);
}

function typeahead(shell: HTMLElement, key: string): HTMLElement | null {
  const now = Date.now();
  const last = Number(shell.dataset.sbTypeaheadAt ?? 0);
  const previous =
    now - last <= TYPEAHEAD_RESET_MS ? (shell.dataset.sbTypeahead ?? "") : "";
  const combined = `${previous}${key}`.toLocaleLowerCase();
  const first = combined.charAt(0);
  let repeated = true;
  for (let index = 1; index < combined.length; index += 1) {
    if (combined.charAt(index) !== first) repeated = false;
  }
  const query = repeated ? key.toLocaleLowerCase() : combined;
  shell.dataset.sbTypeahead = combined;
  shell.dataset.sbTypeaheadAt = String(now);

  const options = optionsOf(shell);
  const current =
    activeOption(shell) ??
    options.find((option) => option.getAttribute("aria-selected") === "true") ??
    null;
  const start = current === null ? 0 : options.indexOf(current) + 1;
  for (let offset = 0; offset < options.length; offset += 1) {
    const item = options[(start + offset) % options.length];
    if (item?.textContent.trim().toLocaleLowerCase().startsWith(query)) {
      return item;
    }
  }
  return null;
}

function sync(shell: HTMLElement): void {
  const source = sourceOf(shell);
  const trigger = triggerOf(shell);
  if (source === null || trigger === null) return;
  const selected = source.multiple
    ? Array.from(source.options).filter((option) => option.selected)
    : [source.options.item(source.selectedIndex)].filter(
        (option): option is HTMLOptionElement => option !== null,
      );
  trigger.textContent = selected.map(optionText).join(", ");
  trigger.disabled = source.disabled;
  queryAll(shell, ".sb-select-option").forEach((item) => {
    const index = Number(item.dataset.sbIndex);
    const option = Number.isInteger(index) ? source.options.item(index) : null;
    item.setAttribute("aria-selected", String(option?.selected === true));
  });
  if (source.validity.valid && trigger.hasAttribute(NATIVE_INVALID_ATTR)) {
    trigger.removeAttribute(NATIVE_INVALID_ATTR);
    const authored = source.getAttribute("aria-invalid");
    if (authored === null) trigger.removeAttribute("aria-invalid");
    else trigger.setAttribute("aria-invalid", authored);
  }
}

function dispatchSelection(source: HTMLSelectElement): void {
  source.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
  source.dispatchEvent(new Event("change", { bubbles: true, composed: true }));
}

function choose(shell: HTMLElement, item: HTMLElement): void {
  const source = sourceOf(shell);
  const index = Number(item.dataset.sbIndex);
  const option = source?.options.item(index) ?? null;
  const group = option?.closest("optgroup");
  if (
    source === null ||
    option === null ||
    option.disabled ||
    (group instanceof HTMLOptGroupElement && group.disabled)
  ) {
    return;
  }
  if (source.multiple) option.selected = !option.selected;
  else source.selectedIndex = index;
  sync(shell);
  dispatchSelection(source);
  if (!source.multiple) close(shell, true);
}

function installResetSync(source: HTMLSelectElement): void {
  const form = source.form;
  if (form === null || resetForms.has(form)) return;
  resetForms.add(form);
  form.addEventListener("reset", () => {
    queueMicrotask(() => {
      for (const item of form.querySelectorAll<HTMLSelectElement>(
        "select.sb-select-source",
      )) {
        const shell = item.closest<HTMLElement>(".sb-select-shell");
        if (shell === null) continue;
        const trigger = triggerOf(shell);
        if (trigger !== null) {
          trigger.removeAttribute(NATIVE_INVALID_ATTR);
          const authored = item.getAttribute("aria-invalid");
          if (authored === null) trigger.removeAttribute("aria-invalid");
          else trigger.setAttribute("aria-invalid", authored);
        }
        sync(shell);
      }
    });
  });
}

function enhance(source: HTMLSelectElement): void {
  if (source.classList.contains("sb-select-source")) return;
  if (source.matches("[data-sb-native]")) {
    source.classList.add("sb-select", "sb-select-native");
    return;
  }
  const parent = source.parentNode;
  if (parent === null) return;
  const sourceRoot = source.getRootNode();
  const restoreFocus =
    (sourceRoot instanceof Document || sourceRoot instanceof ShadowRoot) &&
    sourceRoot.activeElement === source;

  selectSequence += 1;
  const shell = source.ownerDocument.createElement("div");
  shell.className = "sb-select-shell";
  const trigger = source.ownerDocument.createElement("button");
  trigger.type = "button";
  trigger.className = "sb-select-trigger";
  trigger.id = `sb-select-trigger-${String(selectSequence)}`;
  trigger.setAttribute("role", "combobox");
  trigger.setAttribute("aria-haspopup", "listbox");
  trigger.setAttribute("aria-expanded", "false");
  const list = source.ownerDocument.createElement("div");
  list.className = "sb-select-list";
  list.id = `sb-select-list-${String(selectSequence)}`;
  list.setAttribute("role", "listbox");
  if (source.multiple) list.setAttribute("aria-multiselectable", "true");
  list.setAttribute("popover", "manual");
  list.hidden = true;
  trigger.setAttribute("aria-controls", list.id);

  buildSelectOptions(source, list, selectSequence);

  for (const attribute of source.getAttributeNames()) {
    if (!attribute.startsWith("aria-") || attribute === "aria-hidden") continue;
    const value = source.getAttribute(attribute);
    if (value !== null) trigger.setAttribute(attribute, value);
  }
  if (source.required) trigger.setAttribute("aria-required", "true");
  if (source.title !== "") {
    trigger.title = source.title;
    source.removeAttribute("title");
  }
  for (const label of Array.from(source.labels)) label.htmlFor = trigger.id;
  parent.insertBefore(shell, source);
  source.classList.remove("sb-select");
  source.classList.add("sb-select-source");
  source.tabIndex = -1;
  source.setAttribute("aria-hidden", "true");
  shell.append(source, trigger, list);
  source.addEventListener("invalid", (event) => {
    event.preventDefault();
    trigger.setAttribute(NATIVE_INVALID_ATTR, "");
    trigger.setAttribute("aria-invalid", "true");
    trigger.focus();
  });
  installResetSync(source);
  sync(shell);
  if (restoreFocus) trigger.focus();
}

behaviour("click", "*", (_element, event) => {
  if (openShell !== null && !openShell.isConnected) openShell = null;
  if (openShell !== null && !event.composedPath().includes(openShell)) {
    close(openShell, false);
  }
});

behaviour("click", "label[for]", (label, event) => {
  const scope = label.getRootNode();
  if (!(scope instanceof Document || scope instanceof ShadowRoot)) return;
  const target = scope.getElementById(label.getAttribute("for") ?? "");
  if (!(target instanceof HTMLButtonElement)) return;
  if (!target.classList.contains("sb-select-trigger")) return;
  const from = event.composedPath()[0];
  if (from instanceof Node && target.contains(from)) return;
  event.preventDefault();
  target.click();
});

behaviour("click", ".sb-select-trigger", (trigger) => {
  const shell = trigger.closest<HTMLElement>(".sb-select-shell");
  if (shell === null) return;
  if (shell.hasAttribute("data-open")) close(shell, true);
  else open(shell);
});

behaviour("click", ".sb-select-option", (item, event) => {
  if (item.getAttribute("aria-disabled") === "true") return;
  const shell = item.closest<HTMLElement>(".sb-select-shell");
  if (shell === null) return;
  event.preventDefault();
  setActive(shell, item);
  choose(shell, item);
});

behaviour("keydown", ".sb-select-trigger", (element, event) => {
  if (!(event instanceof KeyboardEvent)) return;
  const shell = element.closest<HTMLElement>(".sb-select-shell");
  if (shell === null) return;
  if (event.key === "Tab") {
    close(shell, false);
    return;
  }
  if (event.key === "Escape") {
    if (!shell.hasAttribute("data-open")) return;
    event.preventDefault();
    event.stopPropagation();
    close(shell, true);
    return;
  }
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    const active = activeOption(shell);
    if (shell.hasAttribute("data-open") && active !== null)
      choose(shell, active);
    else open(shell);
    return;
  }
  if (event.key === "Home" || event.key === "End") {
    event.preventDefault();
    if (!shell.hasAttribute("data-open")) {
      open(shell, event.key === "Home" ? "first" : "last");
    } else {
      activateOption(shell, event.key === "Home" ? "first" : "last");
    }
    return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    if (!shell.hasAttribute("data-open")) open(shell);
    else moveActive(shell, event.key === "ArrowDown" ? 1 : -1);
    return;
  }
  if (
    event.key.length !== 1 ||
    event.altKey ||
    event.ctrlKey ||
    event.metaKey
  ) {
    return;
  }
  const match = typeahead(shell, event.key);
  if (match === null) return;
  event.preventDefault();
  if (shell.hasAttribute("data-open")) setActive(shell, match);
  else choose(shell, match);
});

behaviour("focusout", ".sb-select-shell", (shell, event) => {
  const next = event instanceof FocusEvent ? event.relatedTarget : null;
  if (!(next instanceof Node) || !shell.contains(next)) close(shell, false);
});

behaviour("input", ".sb-select-source", (source) => {
  const shell = source.closest<HTMLElement>(".sb-select-shell");
  if (shell !== null) sync(shell);
});

behaviour("change", ".sb-select-source", (source) => {
  const shell = source.closest<HTMLElement>(".sb-select-shell");
  if (shell !== null) sync(shell);
});

behaviour("scroll", "*", (_element, event) => {
  if (
    openShell !== null &&
    !event
      .composedPath()
      .some(
        (node) =>
          node instanceof HTMLElement &&
          node.classList.contains("sb-select-list"),
      )
  ) {
    close(openShell, false);
  }
});

onSync((root) => {
  for (const source of root.querySelectorAll<HTMLSelectElement>("select")) {
    enhance(source);
  }
});
