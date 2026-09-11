// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test } from "vitest";

import { installKitBehaviour, syncKit } from "./index";

let teardown: (() => void) | null = null;
let host: HTMLDivElement;
let root: ShadowRoot;

/**
 * Renders markup inside a real shadow root attached to the document.
 *
 * That is the point of most of these tests: a plugin's markup lives in an
 * open shadow root, where `event.target` is retargeted to the host. Testing
 * against a plain div would pass while the real thing is broken.
 */
function render(html: string): ShadowRoot {
  root.innerHTML = html;
  syncKit(root);
  return root;
}

/**
 * The one element matching `selector`, typed.
 *
 * A missing element is a broken test, not a null to handle — throwing here
 * names the selector instead of failing later on a property of null.
 */
function one(tree: ParentNode, selector: string): HTMLElement {
  const found = tree.querySelector<HTMLElement>(selector);
  if (found === null) throw new Error(`no element matches ${selector}`);
  return found;
}

/** The same for a form control, whose value and checked state tests read. */
function oneInput(tree: ParentNode, selector: string): HTMLInputElement {
  const found = one(tree, selector);
  if (!(found instanceof HTMLInputElement)) {
    throw new Error(`${selector} is not an input`);
  }
  return found;
}

/** Every input matching `selector`, in document order. */
function inputsOf(tree: ParentNode, selector: string): HTMLInputElement[] {
  return Array.from(tree.querySelectorAll<HTMLInputElement>(selector));
}

/** Dispatches a composed event the way a real interaction does. */
function fire(element: Element, type: string, init: EventInit = {}): void {
  element.dispatchEvent(
    new Event(type, { bubbles: true, composed: true, ...init }),
  );
}

function click(element: Element): void {
  element.dispatchEvent(
    new MouseEvent("click", { bubbles: true, composed: true }),
  );
}

function press(element: Element, key: string): void {
  element.dispatchEvent(
    new KeyboardEvent("keydown", { key, bubbles: true, composed: true }),
  );
}

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = host.attachShadow({ mode: "open" });
  teardown = installKitBehaviour();
});

afterEach(() => {
  teardown?.();
  teardown = null;
  host.remove();
});

test("delegation reaches elements inside a plugin shadow root", () => {
  // event.target would be the host here; only composedPath() finds the input.
  const tree = render(`
    <div data-sb-number>
      <button data-sb-number-down></button>
      <input type="number" value="5" min="0" max="10">
      <button data-sb-number-up></button>
    </div>`);
  const input = oneInput(tree, "input");
  click(one(tree, "[data-sb-number-up]"));
  expect(input.value).toBe("6");
});

test("the stepper disables the button that would leave the bounds", () => {
  const tree = render(`
    <div data-sb-number>
      <button data-sb-number-down></button>
      <input type="number" value="10" min="0" max="10">
      <button data-sb-number-up></button>
    </div>`);
  const up = one(tree, "[data-sb-number-up]");
  const down = one(tree, "[data-sb-number-down]");
  expect(up.matches(":disabled")).toBe(true);
  expect(down.matches(":disabled")).toBe(false);
});

const TABLE = `
  <input type="search" data-field="q" data-sb-filter="#t">
  <p data-sb-filter-count="#t"></p>
  <table id="t">
    <thead><tr>
      <th data-sb-sort><button type="button">Name</button></th>
      <th data-sb-sort="number"><button type="button">Amount</button></th>
    </tr></thead>
    <tbody>
      <tr><td>Charlie</td><td>$90.00</td></tr>
      <tr><td>alice</td><td>$1,250.00</td></tr>
      <tr><td>Bob</td><td>$200.00</td></tr>
    </tbody>
  </table>`;

function rowTexts(tree: ParentNode, column = 0): string[] {
  return Array.from(tree.querySelectorAll("tbody tr")).map(
    (row) => row.querySelectorAll("td")[column]?.textContent ?? "",
  );
}

test("clicking a header sorts, and clicking again reverses", () => {
  const tree = render(TABLE);
  const header = one(tree, "th[data-sb-sort]");
  click(one(header, "button"));
  expect(rowTexts(tree)).toEqual(["alice", "Bob", "Charlie"]);
  expect(header.getAttribute("aria-sort")).toBe("ascending");
  click(one(header, "button"));
  expect(rowTexts(tree)).toEqual(["Charlie", "Bob", "alice"]);
  expect(header.getAttribute("aria-sort")).toBe("descending");
});

test("a numeric column sorts by value, not by string", () => {
  const tree = render(TABLE);
  const headers = tree.querySelectorAll("th[data-sb-sort]");
  click(one(headers[1] ?? tree, "button"));
  // Lexically "$1,250.00" would come first — currency and separators are
  // stripped before comparing.
  expect(rowTexts(tree, 1)).toEqual(["$90.00", "$200.00", "$1,250.00"]);
  // aria-sort marks exactly one column.
  expect(tree.querySelectorAll("th[aria-sort]")).toHaveLength(1);
});

test("the filter hides non-matching rows and reports the count", () => {
  const tree = render(TABLE);
  const search = oneInput(tree, "[data-sb-filter]");
  search.value = "bo";
  fire(search, "input");
  const visible = Array.from(
    tree.querySelectorAll<HTMLElement>("tbody tr"),
  ).filter((row) => !row.hidden);
  expect(visible).toHaveLength(1);
  expect(visible[0]?.textContent).toContain("Bob");
});

test("a re-render re-applies the active filter", () => {
  const tree = render(TABLE);
  const search = oneInput(tree, "[data-sb-filter]");
  search.value = "bo";
  fire(search, "input");
  // What PluginContent does every render: fresh markup, restored field value.
  render(TABLE);
  const restored = oneInput(tree, "[data-sb-filter]");
  restored.value = "bo";
  syncKit(tree);
  const visible = Array.from(
    tree.querySelectorAll<HTMLElement>("tbody tr"),
  ).filter((row) => !row.hidden);
  expect(visible).toHaveLength(1);
});

test("a dropdown opens, and any other click dismisses it", () => {
  const tree = render(`
    <div data-sb-dropdown>
      <button data-sb-dropdown-toggle>Menu</button>
      <div class="sb-dropdown__menu"><a role="menuitem">One</a></div>
    </div>
    <p id="elsewhere">text</p>`);
  const dropdown = one(tree, "[data-sb-dropdown]");
  click(one(tree, "[data-sb-dropdown-toggle]"));
  expect(dropdown.classList.contains("is-open")).toBe(true);
  expect(
    tree
      .querySelector("[data-sb-dropdown-toggle]")
      ?.getAttribute("aria-expanded"),
  ).toBe("true");
  click(one(tree, "#elsewhere"));
  expect(dropdown.classList.contains("is-open")).toBe(false);
});

test("a multiselect builds one pill per checked box", () => {
  const tree = render(`
    <div data-sb-multiselect>
      <span class="sb-multiselect__placeholder">Tags</span>
      <button class="sb-multiselect__toggle"></button>
      <div class="sb-multiselect__panel">
        <label><input type="checkbox" name="t" value="a" checked> Alpha</label>
        <label><input type="checkbox" name="t" value="b"> Beta</label>
      </div>
    </div>`);
  expect(tree.querySelectorAll(".sb-multiselect__pill")).toHaveLength(1);
  const beta = inputsOf(tree, "input")[1] ?? oneInput(tree, "input");
  beta.checked = true;
  fire(beta, "change");
  expect(tree.querySelectorAll(".sb-multiselect__pill")).toHaveLength(2);
  // The toggle keeps an accessible name once the placeholder is hidden.
  const toggle = one(tree, ".sb-multiselect__toggle");
  expect(toggle.getAttribute("aria-label")).toContain("2");
});

test("a tag input turns Enter into a chip and syncs the hidden field", () => {
  const tree = render(`
    <div data-sb-taginput="tags">
      <input class="sb-taginput__input">
    </div>`);
  const input = oneInput(tree, ".sb-taginput__input");
  input.value = "rust";
  press(input, "Enter");
  input.value = "RUST"; // duplicates are refused case-insensitively
  press(input, "Enter");
  expect(tree.querySelectorAll(".sb-chip")).toHaveLength(1);
  const hidden = oneInput(tree, 'input[type="hidden"]');
  expect(hidden.value).toBe("rust");
  // It reaches the plugin through the ordinary form contract.
  expect(hidden.dataset.field).toBe("tags");
});

test("an OTP slot advances to the next one as it fills", () => {
  const tree = render(`
    <div data-sb-otp>
      <input maxlength="1"><input maxlength="1"><input maxlength="1">
    </div>`);
  const slots = inputsOf(tree, "input");
  const first = slots[0] ?? oneInput(tree, "input");
  first.value = "7";
  fire(first, "input");
  expect(root.activeElement).toBe(slots[1]);
});

test("a combobox filters its datalist options into a themed listbox", () => {
  const tree = render(`
    <div data-sb-combobox>
      <input list="fruit">
      <datalist id="fruit">
        <option value="Apple"></option>
        <option value="Apricot"></option>
        <option value="Banana"></option>
      </datalist>
    </div>`);
  const input = oneInput(tree, "input");
  // The native popup is suppressed only once the replacement exists.
  expect(input.hasAttribute("list")).toBe(false);
  expect(input.getAttribute("role")).toBe("combobox");
  input.value = "ap";
  input.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
  const options = tree.querySelectorAll(".sb-combobox__option");
  expect(Array.from(options, (o) => o.textContent)).toEqual([
    "Apple",
    "Apricot",
  ]);
});

test("the date picker writes an ISO date into its input", () => {
  const tree = render(`
    <div data-sb-datepicker class="sb-datepicker-field">
      <input value="2026-08-15">
    </div>`);
  const input = oneInput(tree, "input");
  click(input);
  const wrapper = one(tree, "[data-sb-datepicker]");
  expect(wrapper.classList.contains("is-open")).toBe(true);
  const day = one(wrapper, '[data-date="2026-08-20"]');
  click(day);
  expect(input.value).toBe("2026-08-20");
  expect(wrapper.classList.contains("is-open")).toBe(false);
});

test("the range picker marks the span between both endpoints", () => {
  const tree = render(`
    <div data-sb-daterange>
      <input value="2026-08-10"><input value="2026-08-14">
    </div>`);
  const inputs = inputsOf(tree, "input");
  click(inputs[0] as HTMLElement);
  const wrapper = one(tree, "[data-sb-daterange]");
  expect(
    wrapper
      .querySelector('[data-date="2026-08-12"]')
      ?.classList.contains("is-in-range"),
  ).toBe(true);
  // Two picks commit both fields, ordered even when picked backwards.
  click(one(wrapper, '[data-date="2026-08-25"]'));
  click(one(wrapper, '[data-date="2026-08-20"]'));
  expect(inputs.map((input) => input.value)).toEqual([
    "2026-08-20",
    "2026-08-25",
  ]);
});

test("a countdown fills its parts and finishes a past target", () => {
  const tree = render(`
    <div data-sb-countdown="2000-01-01T00:00">
      <span data-sb-countdown-part="days"></span>
      <span data-sb-countdown-part="hours"></span>
      <p data-sb-countdown-done hidden>Done</p>
    </div>`);
  const countdown = one(tree, "[data-sb-countdown]");
  expect(countdown.classList.contains("is-done")).toBe(true);
  expect(
    tree.querySelector<HTMLElement>("[data-sb-countdown-done]")?.hidden,
  ).toBe(false);
  expect(
    tree.querySelector('[data-sb-countdown-part="days"]')?.textContent,
  ).toBe("00");
});

test("the copy button reads the element its selector names", async () => {
  const written: string[] = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: {
      writeText: (text: string) => {
        written.push(text);
        return Promise.resolve();
      },
    },
  });
  const tree = render(`
    <code id="token">sk-abc123</code>
    <button data-sb-copy="#token"></button>`);
  click(one(tree, "[data-sb-copy]"));
  await Promise.resolve();
  expect(written).toEqual(["sk-abc123"]);
});

test("a copy selector that names nothing is ignored, not thrown", () => {
  const tree = render('<button data-sb-copy="#missing"></button>');
  expect(() => {
    click(one(tree, "[data-sb-copy]"));
  }).not.toThrow();
});
