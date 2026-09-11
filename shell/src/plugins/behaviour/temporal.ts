/** Date/time fields compose the existing calendar with a small time editor. */
import { t } from "../../i18n/t";
import { formatDate, toISO } from "./dates";
import { behaviour, onSync } from "./delegate";

let sequence = 0;

function button(
  className: string,
  label: string,
  icon?: string,
): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.className = className;
  if (icon) {
    const symbol = document.createElement("span");
    symbol.dataset.lucide = icon;
    symbol.setAttribute("aria-hidden", "true");
    element.append(symbol);
  }
  const text = document.createElement("span");
  text.textContent = label;
  element.append(text);
  return element;
}

function closeTime(shell: HTMLElement, refocus = false): void {
  const panel = shell.querySelector<HTMLElement>(".sb-temporal-time");
  if (panel) {
    panel.hidden = true;
    for (const input of panel.querySelectorAll("input")) input.disabled = true;
  }
  const trigger = shell.querySelector<HTMLElement>(".sb-temporal-time-trigger");
  trigger?.setAttribute("aria-expanded", "false");
  if (refocus) trigger?.focus();
}

function enhance(source: HTMLInputElement): void {
  if (
    !["date", "time", "datetime-local"].includes(source.type) ||
    source.closest(".sb-temporal")
  )
    return;
  const tree = source.getRootNode();
  const focused =
    (tree instanceof ShadowRoot
      ? tree.activeElement
      : document.activeElement) === source;
  const shell = document.createElement("div");
  shell.className = "sb-temporal";
  source.before(shell);
  shell.append(source);
  source.classList.add("sb-temporal-source");
  source.tabIndex = -1;
  source.setAttribute("aria-hidden", "true");
  const dateField = document.createElement("div");
  dateField.className = "sb-datepicker-field";
  dateField.setAttribute("data-sb-datepicker", "");
  const dateValue = document.createElement("input");
  dateValue.type = "text";
  dateValue.hidden = true;
  const dateTrigger = button(
    "sb-temporal-date-trigger",
    t("kit.chooseDate"),
    "calendar-days",
  );
  dateTrigger.setAttribute("data-sb-datepicker-toggle", "");
  dateField.append(dateValue, dateTrigger);
  const timeTrigger = button(
    "sb-temporal-time-trigger",
    t("kit.chooseTime"),
    "clock",
  );
  if (source.type !== "time") shell.append(dateField);
  if (source.type !== "date") shell.append(timeTrigger);
  const clear = button("sb-temporal-clear", "", "x");
  clear.setAttribute("aria-label", t("kit.clearDateTime"));
  shell.append(clear);
  const primary = source.type === "time" ? timeTrigger : dateTrigger;
  primary.id = `sb-temporal-${String(++sequence)}`;
  // As with themed selects, the native input owns validation and wire values.
  // The labelled buttons own accessible interaction, including invalid focus.
  for (const label of Array.from(source.labels ?? []))
    label.htmlFor = primary.id;
  for (const trigger of [dateTrigger, timeTrigger]) {
    trigger.disabled = source.disabled || source.readOnly;
    trigger.setAttribute("aria-haspopup", "dialog");
    trigger.setAttribute("aria-expanded", "false");
    for (const attr of [
      "aria-label",
      "aria-labelledby",
      "aria-describedby",
      "aria-invalid",
      "title",
    ]) {
      const value = source.getAttribute(attr);
      if (value !== null) trigger.setAttribute(attr, value);
    }
  }
  clear.disabled = source.disabled || source.readOnly;
  const error = document.createElement("p");
  error.className = "sb-field__error sb-temporal-error";
  error.id = `${primary.id}-error`;
  error.setAttribute("aria-live", "polite");
  error.hidden = true;
  shell.append(error);
  function validation(): void {
    error.textContent = source.validationMessage;
    error.hidden = source.validity.valid;
    for (const trigger of [dateTrigger, timeTrigger]) {
      trigger.setAttribute(
        "aria-invalid",
        source.validity.valid
          ? (source.getAttribute("aria-invalid") ?? "false")
          : "true",
      );
      trigger.setAttribute(
        "aria-describedby",
        [source.getAttribute("aria-describedby"), error.id]
          .filter(Boolean)
          .join(" "),
      );
    }
  }
  function timeValue(): string {
    return source.type === "time"
      ? source.value
      : (source.value.split("T")[1] ?? "");
  }
  function sync(): void {
    const date =
      source.type === "time" ? "" : (source.value.split("T")[0] ?? "");
    dateValue.value = date;
    const dateLabel = dateTrigger.lastElementChild;
    if (dateLabel)
      dateLabel.textContent =
        date === ""
          ? t("kit.chooseDate")
          : formatDate(new Date(`${date}T12:00:00`));
    const timeLabel = timeTrigger.lastElementChild;
    if (timeLabel) timeLabel.textContent = timeValue() || t("kit.chooseTime");
    clear.hidden = source.value === "";
    shell.toggleAttribute("data-empty", source.value === "");
    if (!error.hidden || source.validity.valid) validation();
  }
  function commit(value: string): void {
    source.value = value;
    source.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
    source.dispatchEvent(
      new Event("change", { bubbles: true, composed: true }),
    );
    validation();
  }
  dateValue.addEventListener("change", () => {
    const now = new Date();
    const time =
      timeValue() ||
      `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
    commit(
      source.type === "date" ? dateValue.value : `${dateValue.value}T${time}`,
    );
  });
  clear.addEventListener("click", () => {
    dateField.classList.remove("is-open");
    dateTrigger.setAttribute("aria-expanded", "false");
    closeTime(shell);
    commit("");
    primary.focus();
  });
  source.addEventListener("invalid", (event) => {
    event.preventDefault();
    validation();
    primary.focus();
  });
  source.addEventListener("input", sync);
  source.addEventListener("change", sync);
  source.addEventListener("focus", () => {
    primary.focus();
  });
  source.form?.addEventListener("reset", () => {
    queueMicrotask(() => {
      dateField.classList.remove("is-open");
      dateTrigger.setAttribute("aria-expanded", "false");
      closeTime(shell);
      sync();
    });
  });

  if (source.type !== "date") {
    const panel = document.createElement("div");
    panel.className = "sb-temporal-time";
    panel.hidden = true;
    panel.setAttribute("role", "dialog");
    panel.setAttribute("aria-label", t("kit.chooseTime"));
    const inputs = ["hour", "minute"].map((part, index) => {
      const label = document.createElement("label");
      label.className = "sb-temporal-unit";
      const caption = document.createElement("span");
      caption.textContent = t(`kit.${part}`);
      const input = document.createElement("input");
      input.type = "number";
      input.className = `sb-input sb-temporal-${part}`;
      input.min = "0";
      input.max = index === 0 ? "23" : "59";
      input.step = "1";
      input.required = true;
      input.disabled = true;
      input.inputMode = "numeric";
      label.append(caption, input);
      panel.append(label);
      return input;
    });
    const apply = button(
      "sb-btn sb-btn-primary sb-temporal-apply",
      t("kit.applyTime"),
    );
    panel.append(apply);
    shell.append(panel);
    timeTrigger.addEventListener("click", () => {
      if (!panel.hidden) {
        closeTime(shell);
        return;
      }
      const now = new Date();
      const parts = timeValue().split(":");
      inputs.forEach((input, index) => {
        input.disabled = false;
        input.value =
          (parts[index] === "" ? undefined : parts[index]) ??
          String(index === 0 ? now.getHours() : now.getMinutes()).padStart(
            2,
            "0",
          );
      });
      panel.hidden = false;
      timeTrigger.setAttribute("aria-expanded", "true");
      inputs[0]?.focus();
      inputs[0]?.select();
    });
    function applyTime(): void {
      if (inputs.some((input) => !input.reportValidity())) return;
      const time = inputs
        .map((input) => input.value.padStart(2, "0"))
        .join(":");
      const date =
        source.value === ""
          ? toISO(new Date())
          : (source.value.split("T")[0] ?? "");
      commit(source.type === "time" ? time : `${date}T${time}`);
      closeTime(shell, true);
    }
    apply.addEventListener("click", applyTime);
    panel.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        event.stopPropagation();
        applyTime();
      }
    });
  }
  sync();
  if (focused) primary.focus();
}

onSync((root) => {
  for (const source of root.querySelectorAll<HTMLInputElement>(
    "input[data-sb-temporal]",
  ))
    enhance(source);
});

behaviour("click", "*", (element) => {
  const scope = element.getRootNode();
  if (!(scope instanceof ShadowRoot || scope instanceof Document)) return;
  for (const shell of scope.querySelectorAll<HTMLElement>(".sb-temporal")) {
    if (
      !element.closest(".sb-temporal-time, .sb-temporal-time-trigger") ||
      !shell.contains(element)
    )
      closeTime(shell);
  }
});
behaviour<KeyboardEvent>("keydown", ".sb-temporal", (shell, event) => {
  if (
    event.key !== "Escape" ||
    shell.querySelector<HTMLElement>(".sb-temporal-time")?.hidden !== false
  )
    return;
  event.preventDefault();
  event.stopPropagation();
  closeTime(shell, true);
});
behaviour<FocusEvent>("focusout", ".sb-temporal", (shell, event) => {
  if (
    event.relatedTarget instanceof Element &&
    !shell.contains(event.relatedTarget)
  )
    closeTime(shell);
});
