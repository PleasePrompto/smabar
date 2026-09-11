/**
 * Multi-value form controls: multiselect, tag input and segmented code
 * input. None of the three has a native equivalent.
 *
 * All of them stay real form controls underneath — the multiselect drives
 * native checkboxes, the tag input syncs a hidden input — so a plugin reads
 * them through the ordinary `data-field` form contract.
 */
import { t } from "../../i18n/t";

import { behaviour, onSync, queryAll } from "./delegate";

/** The tree a plugin's markup lives in. */
function scopeOf(element: Element): ParentNode | null {
  const root = element.getRootNode();
  return root instanceof ShadowRoot || root instanceof Document ? root : null;
}

/** Fills a label template that carries a `{value}` placeholder. */
function label(key: string, value: string): string {
  return t(key).replace("{value}", value);
}

function multiselectToggle(root: ParentNode): HTMLElement | null {
  return root.querySelector(".sb-multiselect__toggle");
}

function multiselectBoxes(root: ParentNode): HTMLInputElement[] {
  return Array.from(
    root.querySelectorAll<HTMLInputElement>(
      '.sb-multiselect__panel input[type="checkbox"]',
    ),
  );
}

function setMultiselectOpen(root: HTMLElement, open: boolean): void {
  root.classList.toggle("is-open", open);
  multiselectToggle(root)?.setAttribute("aria-expanded", String(open));
}

function closeMultiselects(
  scope: ParentNode,
  except: HTMLElement | null,
): void {
  for (const root of queryAll(scope, "[data-sb-multiselect].is-open")) {
    if (root !== except) setMultiselectOpen(root, false);
  }
}

/**
 * Rebuilds the pills from the checked boxes.
 *
 * The checkboxes are the state; pills are a view of them. That is what keeps
 * the control a real form control rather than a re-implementation of one.
 */
function syncMultiselect(root: HTMLElement): void {
  const toggle = multiselectToggle(root);
  if (toggle === null) return;
  for (const pill of queryAll(root, ".sb-multiselect__pill")) pill.remove();
  let count = 0;
  multiselectBoxes(root).forEach((box, index) => {
    if (!box.checked) return;
    count += 1;
    const text = (box.closest("label")?.textContent ?? box.value).trim();
    const pill = document.createElement("span");
    pill.className = "sb-chip sb-multiselect__pill";
    pill.append(text);
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "sb-multiselect__remove";
    remove.setAttribute("aria-label", label("kit.remove", text));
    remove.dataset.sbIndex = String(index);
    remove.disabled = box.disabled;
    remove.textContent = "×";
    pill.append(remove);
    toggle.before(pill);
  });
  // CSS hides the placeholder once pills exist, so the toggle has to carry
  // the field name itself: aria-label wins over <label for> in the accessible
  // name computation (WCAG 2.5.3).
  const named =
    (toggle.id !== ""
      ? scopeOf(toggle)?.querySelector(`label[for="${CSS.escape(toggle.id)}"]`)
      : null) ?? root.querySelector(".sb-multiselect__placeholder");
  const base = (named?.textContent ?? "").trim();
  if (count > 0) {
    const suffix = t("kit.selectedCount").replace("{count}", String(count));
    toggle.setAttribute(
      "aria-label",
      base === "" ? suffix : `${base}, ${suffix}`,
    );
  } else {
    toggle.removeAttribute("aria-label");
  }
}

behaviour("click", "*", (element) => {
  const scope = scopeOf(element);
  if (scope === null) return;
  const root = element.closest<HTMLElement>("[data-sb-multiselect]");
  closeMultiselects(scope, root);
  if (root === null) return;

  const remove = element.closest<HTMLElement>(".sb-multiselect__remove");
  if (remove !== null) {
    const box = multiselectBoxes(root)[Number(remove.dataset.sbIndex)];
    if (box !== undefined) {
      box.checked = false;
      box.dispatchEvent(new Event("change", { bubbles: true }));
    }
    multiselectToggle(root)?.focus();
    return;
  }
  // A click anywhere on the field but the panel toggles the disclosure.
  if (element.closest(".sb-multiselect__panel") === null) {
    setMultiselectOpen(root, !root.classList.contains("is-open"));
    multiselectToggle(root)?.focus();
  }
});

behaviour("change", '[data-sb-multiselect] input[type="checkbox"]', (box) => {
  const root = box.closest<HTMLElement>("[data-sb-multiselect]");
  if (root !== null) syncMultiselect(root);
});

behaviour("keydown", "[data-sb-multiselect]", (root, event) => {
  if (!(event instanceof KeyboardEvent) || event.key !== "Escape") return;
  if (!root.classList.contains("is-open")) return;
  event.preventDefault();
  setMultiselectOpen(root, false);
  multiselectToggle(root)?.focus();
});

// Tabbing out closes. A click on non-focusable space gives a null
// relatedTarget and is left to the click handler above.
behaviour("focusout", "[data-sb-multiselect]", (root, event) => {
  const next = event instanceof FocusEvent ? event.relatedTarget : null;
  if (next instanceof Element && !root.contains(next)) {
    setMultiselectOpen(root, false);
  }
});

/** A chip's value: its own text, ignoring the close button. */
function chipValue(chip: Element): string {
  let text = "";
  for (const node of Array.from(chip.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE) text += node.textContent ?? "";
  }
  return text.trim();
}

function tagValues(wrapper: ParentNode): string[] {
  return queryAll(wrapper, ".sb-chip").map(chipValue);
}

/** The hidden input carrying the tags, created on first need. */
function tagStore(wrapper: HTMLElement): HTMLInputElement | null {
  const existing = wrapper.querySelector('input[type="hidden"]');
  if (existing instanceof HTMLInputElement) return existing;
  const name = wrapper.dataset.sbTaginput;
  if (name === undefined || name === "") return null;
  const hidden = document.createElement("input");
  hidden.type = "hidden";
  hidden.name = name;
  // The form contract reads data-field, so the tags reach the plugin the
  // same way every other field does.
  hidden.dataset.field = name;
  wrapper.append(hidden);
  return hidden;
}

/** Mirrors the chips into the hidden input as a comma-separated list. */
function syncTags(wrapper: HTMLElement): void {
  const hidden = tagStore(wrapper);
  if (hidden === null) return;
  const next = tagValues(wrapper).join(",");
  if (hidden.value === next) return;
  hidden.value = next;
  hidden.dispatchEvent(new Event("change", { bubbles: true }));
}

/** Adds a chip. Returns false for blanks and case-insensitive duplicates. */
function addTag(wrapper: HTMLElement, raw: string): boolean {
  const value = raw.trim();
  if (value === "") return false;
  const lower = value.toLowerCase();
  if (tagValues(wrapper).some((existing) => existing.toLowerCase() === lower)) {
    return false;
  }
  const chip = document.createElement("span");
  chip.className = "sb-chip";
  chip.append(value);
  const close = document.createElement("button");
  close.type = "button";
  close.className = "sb-chip__close";
  close.setAttribute("aria-label", label("kit.remove", value));
  close.textContent = "×";
  chip.append(close);
  const input = wrapper.querySelector(".sb-taginput__input");
  if (input !== null) input.before(chip);
  else wrapper.append(chip);
  syncTags(wrapper);
  return true;
}

behaviour("keydown", ".sb-taginput__input", (input, event) => {
  const wrapper = input.closest<HTMLElement>("[data-sb-taginput]");
  if (wrapper === null || !(input instanceof HTMLInputElement)) return;
  if (!(event instanceof KeyboardEvent) || event.isComposing) return;

  if (event.key === "Enter" || event.key === ",") {
    // Enter on an empty input keeps its default (implicit form submit).
    if (event.key === "Enter" && input.value.trim() === "") return;
    event.preventDefault();
    if (addTag(wrapper, input.value)) input.value = "";
    return;
  }
  if (event.key === "Backspace" && input.value === "") {
    const chips = queryAll(wrapper, ".sb-chip");
    const last = chips[chips.length - 1];
    if (last === undefined) return;
    last.remove();
    syncTags(wrapper);
  }
});

// Pastes and mobile keyboards deliver commas through the value rather than
// keydown: everything before the last comma becomes a chip.
behaviour("input", ".sb-taginput__input", (input) => {
  const wrapper = input.closest<HTMLElement>("[data-sb-taginput]");
  if (wrapper === null || !(input instanceof HTMLInputElement)) return;
  if (!input.value.includes(",")) return;
  const parts = input.value.split(",");
  const rest = parts.pop() ?? "";
  for (const part of parts) addTag(wrapper, part);
  input.value = rest.replace(/^\s+/, "");
});

behaviour("click", "[data-sb-taginput]", (wrapper, event) => {
  const from = event.composedPath()[0];
  if (!(from instanceof HTMLElement)) return;
  const input = wrapper.querySelector<HTMLElement>(".sb-taginput__input");
  const close = from.closest(".sb-chip__close");
  if (close !== null) {
    close.closest(".sb-chip")?.remove();
    syncTags(wrapper);
    input?.focus();
    return;
  }
  // Clicking the empty area focuses the input, like a real text field.
  if (from.closest(".sb-chip") === null) input?.focus();
});

function otpSlots(group: ParentNode): HTMLInputElement[] {
  return Array.from(group.querySelectorAll<HTMLInputElement>("input"));
}

/** Distributes a code across the slots, marking a too-long one invalid. */
function fillOtp(group: HTMLElement, text: string): void {
  const slots = otpSlots(group);
  const truncated = text.length > slots.length;
  slots.forEach((slot, index) => {
    slot.value = text[index] ?? "";
    if (truncated) slot.setAttribute("aria-invalid", "true");
    else slot.removeAttribute("aria-invalid");
  });
  slots[Math.min(text.length, slots.length - 1)]?.focus();
}

behaviour("focusin", "[data-sb-otp] input", (slot) => {
  // Overwrite-first: focusing a filled slot selects it, so typing replaces.
  if (slot instanceof HTMLInputElement) slot.select();
});

behaviour("input", "[data-sb-otp] input", (slot) => {
  const group = slot.closest<HTMLElement>("[data-sb-otp]");
  if (group === null || !(slot instanceof HTMLInputElement)) return;
  const slots = otpSlots(group);
  for (const other of slots) other.removeAttribute("aria-invalid");
  const index = slots.indexOf(slot);
  if (slot.value.length > 1) {
    // Autofill or an IME put several characters into one slot.
    slot.value = slot.value[0] ?? "";
    fillOtp(
      group,
      slots
        .map((one) => one.value)
        .join("")
        .slice(0, slots.length),
    );
    return;
  }
  if (slot.value !== "" && index < slots.length - 1) {
    slots[index + 1]?.focus();
  }
});

behaviour("paste", "[data-sb-otp] input", (slot, event) => {
  const group = slot.closest<HTMLElement>("[data-sb-otp]");
  if (group === null || !(event instanceof ClipboardEvent)) return;
  event.preventDefault();
  const text = (event.clipboardData?.getData("text") ?? "").replace(/\s+/g, "");
  if (text !== "") fillOtp(group, text);
});

behaviour("keydown", "[data-sb-otp] input", (slot, event) => {
  const group = slot.closest<HTMLElement>("[data-sb-otp]");
  if (group === null || !(event instanceof KeyboardEvent)) return;
  if (!(slot instanceof HTMLInputElement)) return;
  const slots = otpSlots(group);
  const index = slots.indexOf(slot);
  if (event.key === "Backspace" && slot.value === "" && index > 0) {
    event.preventDefault();
    const previous = slots[index - 1];
    if (previous !== undefined) {
      previous.value = "";
      previous.focus();
    }
  } else if (event.key === "ArrowLeft" && index > 0) {
    event.preventDefault();
    slots[index - 1]?.focus();
  } else if (event.key === "ArrowRight" && index < slots.length - 1) {
    event.preventDefault();
    slots[index + 1]?.focus();
  }
});

// Server-rendered state (checked boxes, author-written chips) has to reach
// the pills and the hidden input without any interaction.
onSync((root) => {
  for (const multiselect of queryAll(root, "[data-sb-multiselect]")) {
    const toggle = multiselectToggle(multiselect);
    if (toggle !== null && !toggle.hasAttribute("aria-expanded")) {
      toggle.setAttribute("aria-expanded", "false");
    }
    syncMultiselect(multiselect);
  }
  for (const wrapper of queryAll(root, "[data-sb-taginput]")) syncTags(wrapper);
});
