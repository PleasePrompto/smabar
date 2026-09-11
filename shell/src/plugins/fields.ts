/** Form controls supported by the declarative `data-field` contract. */
type Field = HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;

const FIELD_SELECTOR =
  "input[data-field], select[data-field], textarea[data-field]";

/** A range action is committed with its live string value. */
export interface RangeActionIntent {
  action: string;
  value: string;
  field?: string;
  occurrence: number;
}

export function rangeActionIntent(
  target: EventTarget | null,
): RangeActionIntent | null {
  if (!(target instanceof HTMLInputElement) || target.type !== "range") {
    return null;
  }
  const action = target.dataset.action;
  if (action === undefined) return null;
  const field = target.dataset.field;
  const root = target.getRootNode();
  const ranges =
    root instanceof Document || root instanceof DocumentFragment
      ? matchingActionRanges(root, action, field)
      : [target];
  const occurrence = Math.max(0, ranges.indexOf(target));
  return field === undefined
    ? { action, value: target.value, occurrence }
    : { action, value: target.value, field, occurrence };
}

function matchingActionRanges(
  root: ParentNode,
  action: string,
  field: string | undefined,
): HTMLInputElement[] {
  return Array.from(
    root.querySelectorAll<HTMLInputElement>('input[type="range"][data-action]'),
  ).filter(
    (input) => input.dataset.action === action && input.dataset.field === field,
  );
}

/** Finds the same range after provider markup has been rebuilt. */
export function findActionRange(
  root: ParentNode,
  intent: RangeActionIntent,
): HTMLInputElement | undefined {
  return matchingActionRanges(root, intent.action, intent.field)[
    intent.occurrence
  ];
}

/** One field's wire value; checkboxes intentionally travel as booleans. */
function fieldValue(field: Field): string {
  return field instanceof HTMLInputElement && field.type === "checkbox"
    ? String(field.checked)
    : field.value;
}

/** Every named field under one plugin root, in document order. */
function fieldsIn(root: ParentNode): Field[] {
  return Array.from(root.querySelectorAll<Field>(FIELD_SELECTOR));
}

/**
 * Values of all `[data-field]` controls under `root`, keyed by field name.
 * An unchecked radio does not erase its checked sibling's value.
 */
export function collectFieldValues(
  root: ParentNode,
): Record<string, string> | undefined {
  let found = false;
  const fields: Record<string, string> = {};
  for (const field of fieldsIn(root)) {
    const name = field.dataset.field;
    if (name === undefined || name === "") continue;
    if (
      field instanceof HTMLInputElement &&
      field.type === "radio" &&
      !field.checked
    ) {
      continue;
    }
    fields[name] = fieldValue(field);
    found = true;
  }
  return found ? fields : undefined;
}

/** Typed-in state of the `[data-field]` controls under one root. */
export interface FieldSnapshot {
  /** Current value per field name (checkboxes: "true"/"false"). */
  values: Map<string, string>;
  /** Field name of the control that owned focus, if any. */
  focused?: string;
  /** Distinguishes a focused radio from its same-named siblings. */
  focusedValue?: string;
}

/** Structural cross-realm check for Document/ShadowRoot focus support. */
function hasActiveElement(
  root: ParentNode,
): root is ParentNode & Pick<DocumentOrShadowRoot, "activeElement"> {
  return "activeElement" in root;
}

/** Captures form state before a plugin render replaces the DOM. */
export function snapshotFields(root: ParentNode): FieldSnapshot {
  const active = hasActiveElement(root) ? root.activeElement : null;
  const focusedControl =
    active instanceof HTMLElement
      ? active
          .closest(".sb-select-shell, .sb-temporal")
          ?.querySelector<Field>(
            "select[data-field], input[data-sb-temporal][data-field]",
          )
      : null;
  const snapshot: FieldSnapshot = { values: new Map() };
  for (const field of fieldsIn(root)) {
    const name = field.dataset.field;
    if (name === undefined || name === "") continue;
    if (
      !(field instanceof HTMLInputElement) ||
      field.type !== "radio" ||
      field.checked
    ) {
      snapshot.values.set(name, fieldValue(field));
    }
    if (field === active || field === focusedControl) {
      snapshot.focused = name;
      if (field instanceof HTMLInputElement && field.type === "radio") {
        snapshot.focusedValue = field.value;
      }
    }
  }
  return snapshot;
}

/** Whether fresh plugin markup explicitly owns a control's next value. */
function hasAuthoredValue(field: Field): boolean {
  if (field instanceof HTMLSelectElement) {
    return field.querySelector("option[selected]") !== null;
  }
  if (field instanceof HTMLTextAreaElement) return field.defaultValue !== "";
  if (field.type === "checkbox") return field.hasAttribute("checked");
  if (field.type === "radio") {
    const name = field.dataset.field;
    const root = field.getRootNode();
    return (
      name !== undefined &&
      (root instanceof Element ||
        root instanceof Document ||
        root instanceof DocumentFragment) &&
      fieldsIn(root).some(
        (candidate) =>
          candidate instanceof HTMLInputElement &&
          candidate.type === "radio" &&
          candidate.dataset.field === name &&
          candidate.hasAttribute("checked"),
      )
    );
  }
  return field.hasAttribute("value");
}

/** Restores user-owned form state into freshly rendered plugin markup. */
export function restoreFields(root: ParentNode, snapshot: FieldSnapshot): void {
  for (const field of fieldsIn(root)) {
    const name = field.dataset.field;
    if (name === undefined || name === "") continue;
    const saved = snapshot.values.get(name);
    if (saved !== undefined && !hasAuthoredValue(field)) {
      if (field instanceof HTMLInputElement && field.type === "checkbox") {
        field.checked = saved === "true";
      } else if (field instanceof HTMLInputElement && field.type === "radio") {
        field.checked = field.value === saved;
      } else if (
        !(field instanceof HTMLSelectElement) ||
        Array.from(field.options).some((option) => option.value === saved)
      ) {
        field.value = saved;
      }
    }

    const focused =
      name === snapshot.focused &&
      (!(field instanceof HTMLInputElement) ||
        field.type !== "radio" ||
        field.value === snapshot.focusedValue);
    if (!focused) continue;
    field.focus();
    if (
      field instanceof HTMLTextAreaElement ||
      (field instanceof HTMLInputElement && field.selectionStart !== null)
    ) {
      field.setSelectionRange(field.value.length, field.value.length);
    }
  }
}
