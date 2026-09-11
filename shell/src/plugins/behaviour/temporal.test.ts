// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test } from "vitest";
import { collectFieldValues } from "../fields";
import { installKitBehaviour, syncKit } from "./index";

let host: HTMLElement;
let root: ShadowRoot;
let teardown: () => void;
beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = host.attachShadow({ mode: "open" });
  teardown = installKitBehaviour();
});
afterEach(() => {
  teardown();
  host.remove();
});
function one(selector: string): HTMLElement {
  const element = root.querySelector<HTMLElement>(selector);
  if (!element) throw new Error(`Missing ${selector}`);
  return element;
}
function render(type: string, value: string, extra = ""): HTMLInputElement {
  root.innerHTML = `<form><label for="due">Reminder</label><input id="due" data-field="due" data-sb-temporal type="${type}" value="${value}" ${extra}><button type="submit">Save</button></form>`;
  syncKit(root);
  const source = one("#due");
  if (!(source instanceof HTMLInputElement)) throw new Error("Not an input");
  return source;
}
function click(selector: string): void {
  one(selector).dispatchEvent(
    new MouseEvent("click", { bubbles: true, composed: true }),
  );
}
test.each(["date", "datetime-local"])(
  "selecting a %s day closes the calendar and preserves the time",
  (type) => {
    const source = render(
      type,
      type === "date" ? "2026-09-05" : "2026-09-05T14:35",
    );
    click(".sb-temporal-date-trigger");
    click('[data-date="2026-09-18"]');
    expect(source.value).toBe(
      type === "date" ? "2026-09-18" : "2026-09-18T14:35",
    );
    expect(one("[data-sb-datepicker]").classList.contains("is-open")).toBe(
      false,
    );
    expect(root.activeElement).toBe(one(".sb-temporal-date-trigger"));
    expect(collectFieldValues(root)).toEqual({ due: source.value });
  },
);
test.each(["time", "datetime-local"])(
  "confirms %s without submitting the plugin form",
  (type) => {
    const source = render(type, type === "time" ? "14:35" : "2026-09-05T14:35");
    let submits = 0;
    one("form").addEventListener("submit", (e) => {
      e.preventDefault();
      submits++;
    });
    click(".sb-temporal-time-trigger");
    const hour = one(".sb-temporal-hour");
    if (!(hour instanceof HTMLInputElement)) throw new Error("Not an input");
    hour.value = "20";
    click(".sb-temporal-apply");
    expect(source.value).toBe(type === "time" ? "20:35" : "2026-09-05T20:35");
    expect(one(".sb-temporal-time").hidden).toBe(true);
    expect(submits).toBe(0);
  },
);
test("Escape cancels the time draft without reaching the enclosing flyout", () => {
  const source = render("time", "14:35");
  click(".sb-temporal-time-trigger");
  let escaped = false;
  const listener = () => {
    escaped = true;
  };
  window.addEventListener("keydown", listener);
  one(".sb-temporal-hour").dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Escape",
      bubbles: true,
      composed: true,
    }),
  );
  window.removeEventListener("keydown", listener);
  expect(source.value).toBe("14:35");
  expect(escaped).toBe(false);
  expect(one(".sb-temporal-time").hidden).toBe(true);
});
test("native required and bounds validation remain attached to the form source", () => {
  const source = render("date", "", 'required min="2026-09-01"');
  expect(source.checkValidity()).toBe(false);
  expect(root.activeElement).toBe(one(".sb-temporal-date-trigger"));
  // happy-dom does not implement date rangeUnderflow; exercise it in the browser.
  expect(source.min).toBe("2026-09-01");
  source.value = "2026-09-05";
  source.dispatchEvent(new Event("change"));
  expect(source.checkValidity()).toBe(true);
  click(".sb-temporal-clear");
  expect(source.value).toBe("");
});

test("an invalid time draft cannot replace the field or block a later form submission", () => {
  const source = render("time", "14:35");
  click(".sb-temporal-time-trigger");
  const hour = one(".sb-temporal-hour");
  if (!(hour instanceof HTMLInputElement)) throw new Error("Not an input");
  hour.value = "24";
  click(".sb-temporal-apply");
  expect(source.value).toBe("14:35");
  expect(one(".sb-temporal-time").hidden).toBe(false);
  click(".sb-temporal-time-trigger");
  expect(hour.disabled).toBe(true);
  const form = one("form");
  if (!(form instanceof HTMLFormElement)) throw new Error("Not a form");
  expect(form.checkValidity()).toBe(true);
});

test("Escape closes the calendar before the enclosing flyout handles it", () => {
  render("date", "2026-09-05");
  click(".sb-temporal-date-trigger");
  const event = new KeyboardEvent("keydown", {
    key: "Escape",
    bubbles: true,
    composed: true,
    cancelable: true,
  });
  one('[data-date="2026-09-05"]').dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
  expect(one("[data-sb-datepicker]").classList.contains("is-open")).toBe(false);
});
