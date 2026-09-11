// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test } from "vitest";

import { restoreFields, snapshotFields } from "../fields";
import { installKitBehaviour, syncKit } from "./index";

let host: HTMLDivElement;
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

function render(html: string): void {
  root.innerHTML = html;
  syncKit(root);
}

type ElementConstructor<T extends Element> = new () => T;

function one<T extends Element>(
  selector: string,
  kind: ElementConstructor<T>,
): T {
  const element = root.querySelector(selector);
  if (!(element instanceof kind)) throw new Error(`${selector} missing`);
  return element;
}

function click(element: Element): MouseEvent {
  const event = new MouseEvent("click", {
    bubbles: true,
    cancelable: true,
    composed: true,
  });
  element.dispatchEvent(event);
  return event;
}

function press(element: Element, key: string): KeyboardEvent {
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
    composed: true,
  });
  element.dispatchEvent(event);
  return event;
}

test("selects are themed by default while data-sb-native opts out", () => {
  render(`
    <label for="city">City</label>
    <select id="city" class="sb-select" data-field="city" required aria-invalid="true">
      <option value="berlin">Berlin</option>
      <option value="oslo" selected>Oslo</option>
    </select>
    <select id="native" class="sb-select" data-sb-native><option>Native</option></select>`);

  const source = one("select[data-field]", HTMLSelectElement);
  const trigger = one(".sb-select-trigger", HTMLButtonElement);
  expect(source.closest(".sb-select-shell")).not.toBeNull();
  expect(source.classList.contains("sb-select-source")).toBe(true);
  expect(source.hidden).toBe(false);
  expect(source.getAttribute("aria-hidden")).toBe("true");
  expect(source.value).toBe("oslo");
  expect(trigger.textContent).toBe("Oslo");
  expect(trigger.getAttribute("role")).toBe("combobox");
  expect(trigger.getAttribute("aria-required")).toBe("true");
  expect(trigger.getAttribute("aria-invalid")).toBe("true");
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  expect(root.querySelectorAll(".sb-select-option")).toHaveLength(2);
  const label = root.querySelector("label");
  expect(label?.getAttribute("for")).toBe(trigger.id);
  label?.click();
  expect(source.matches(":focus")).toBe(false);
  expect(
    trigger.closest(".sb-select-shell")?.contains(root.activeElement),
  ).toBe(true);
  expect(root.querySelector("#native")?.closest(".sb-select-shell")).toBeNull();
});

test("required themed selects keep native validation focusable and expose the error", () => {
  render(`
    <form>
      <select required>
        <option value="" selected>Choose one</option>
        <option value="team">Team</option>
      </select>
    </form>`);
  const source = one("select", HTMLSelectElement);
  const trigger = one(".sb-select-trigger", HTMLButtonElement);
  const team = root.querySelectorAll<HTMLElement>(".sb-select-option")[1];
  if (team === undefined) throw new Error("team option missing");

  const invalid = new Event("invalid", { cancelable: true });
  source.dispatchEvent(invalid);
  expect(invalid.defaultPrevented).toBe(true);
  expect(trigger.getAttribute("aria-invalid")).toBe("true");
  expect(root.activeElement).toBe(trigger);

  click(trigger);
  click(team);
  expect(source.value).toBe("team");
  expect(trigger.hasAttribute("aria-invalid")).toBe(false);
});

test("complex selects preserve option indexes, groups, and multiple selection", () => {
  render(`
    <select id="complex" multiple size="4">
      <option value="hidden" hidden selected>Hidden</option>
      <optgroup label="Enabled group">
        <option value="one" selected>One</option>
        <option value="blocked" disabled>Blocked</option>
      </optgroup>
      <optgroup label="Disabled group" disabled>
        <option value="grouped">Grouped</option>
      </optgroup>
      <option value="last">Last</option>
    </select>
    <select id="sized" size="2"><option>One</option><option>Two</option></select>`);

  const source = one("#complex", HTMLSelectElement);
  const shell = source.closest<HTMLElement>(".sb-select-shell");
  if (shell === null) throw new Error("complex select shell missing");
  const trigger = shell.querySelector<HTMLButtonElement>(".sb-select-trigger");
  const list = shell.querySelector<HTMLElement>(".sb-select-list");
  if (trigger === null || list === null) throw new Error("select UI missing");
  const groups = Array.from(
    shell.querySelectorAll<HTMLElement>(".sb-select-group"),
  );
  const items = Array.from(
    shell.querySelectorAll<HTMLElement>(".sb-select-option"),
  );

  expect(list.getAttribute("aria-multiselectable")).toBe("true");
  expect(groups.map((group) => group.getAttribute("aria-label"))).toEqual([
    "Enabled group",
    "Disabled group",
  ]);
  expect(groups[0]?.getAttribute("role")).toBe("group");
  expect(groups[1]?.getAttribute("aria-disabled")).toBe("true");
  expect(items.map((item) => item.dataset.sbIndex)).toEqual([
    "1",
    "2",
    "3",
    "4",
  ]);
  expect(items.map((item) => item.textContent)).not.toContain("Hidden");
  expect(items[2]?.getAttribute("aria-disabled")).toBe("true");

  click(trigger);
  click(items[3] ?? trigger);
  expect(source.options.item(4)?.selected).toBe(true);
  expect(trigger.textContent).toBe("Hidden, One, Last");
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  click(items[2] ?? trigger);
  expect(source.options.item(3)?.selected).toBe(false);

  expect(
    one("#sized", HTMLSelectElement).closest(".sb-select-shell"),
  ).not.toBeNull();
  expect(root.querySelectorAll(".sb-select-shell")).toHaveLength(2);
});

test("a themed option updates the real select and emits form events", () => {
  render(`
    <select data-field="city">
      <option value="berlin" selected>Berlin</option>
      <option value="blocked" disabled>Blocked</option>
      <option value="oslo">Oslo</option>
    </select>`);
  const source = one("select", HTMLSelectElement);
  const trigger = one(".sb-select-trigger", HTMLButtonElement);
  const items = root.querySelectorAll<HTMLElement>(".sb-select-option");
  let inputs = 0;
  let changes = 0;
  source.addEventListener("input", () => (inputs += 1));
  source.addEventListener("change", () => (changes += 1));

  click(trigger);
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  click(items[1] ?? trigger);
  expect(source.value).toBe("berlin");
  click(items[2] ?? trigger);
  expect(source.value).toBe("oslo");
  expect(inputs).toBe(1);
  expect(changes).toBe(1);
  expect(trigger.textContent).toBe("Oslo");
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  expect(root.activeElement).toBe(trigger);
});

test("the select keyboard contract covers navigation, commit, dismiss, and Tab", () => {
  render(`
    <select>
      <option value="a" selected>Alpha</option>
      <option value="b" disabled>Blocked</option>
      <option value="c">Charlie</option>
    </select>`);
  const source = one("select", HTMLSelectElement);
  const shell = one(".sb-select-shell", HTMLDivElement);
  const trigger = one(".sb-select-trigger", HTMLButtonElement);
  const items = root.querySelectorAll<HTMLElement>(".sb-select-option");

  trigger.focus();
  expect(press(trigger, "ArrowDown").defaultPrevented).toBe(true);
  expect(shell.hasAttribute("data-open")).toBe(true);
  expect(root.activeElement).toBe(trigger);
  expect(trigger.getAttribute("aria-activedescendant")).toBe(items[0]?.id);
  press(trigger, "ArrowDown");
  expect(trigger.getAttribute("aria-activedescendant")).toBe(items[2]?.id);
  press(trigger, "Home");
  expect(trigger.getAttribute("aria-activedescendant")).toBe(items[0]?.id);
  press(trigger, "End");
  expect(trigger.getAttribute("aria-activedescendant")).toBe(items[2]?.id);
  press(trigger, " ");
  expect(source.value).toBe("c");
  expect(root.activeElement).toBe(trigger);

  press(trigger, "Enter");
  expect(shell.hasAttribute("data-open")).toBe(true);
  expect(press(trigger, "Escape").defaultPrevented).toBe(true);
  expect(shell.hasAttribute("data-open")).toBe(false);
  expect(root.activeElement).toBe(trigger);

  press(trigger, "Enter");
  const tab = press(trigger, "Tab");
  expect(tab.defaultPrevented).toBe(false);
  expect(shell.hasAttribute("data-open")).toBe(false);
});

test("an outside click dismisses the custom select", () => {
  render(
    '<select><option>One</option></select><button id="outside">Outside</button>',
  );
  const trigger = one(".sb-select-trigger", HTMLButtonElement);
  click(trigger);
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  const outside = document.createElement("div");
  document.body.append(outside);
  click(outside);
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  outside.remove();
});

test("the options use the top layer when the popover API exists", () => {
  render("<select><option>One</option></select>");
  const trigger = one(".sb-select-trigger", HTMLButtonElement);
  const list = one(".sb-select-list", HTMLDivElement);
  let shown = 0;
  Object.defineProperty(list, "showPopover", {
    value: () => {
      shown += 1;
    },
  });

  click(trigger);
  expect(shown).toBe(1);
  expect(list.hasAttribute("data-top-layer")).toBe(true);
  expect(list.style.position).toBe("fixed");
});

test("typeahead follows the active option while focus stays on the trigger", () => {
  render(
    "<select><option>Alpha</option><option>Beta</option><option>Bravo</option></select>",
  );
  const source = one("select", HTMLSelectElement);
  const trigger = one(".sb-select-trigger", HTMLButtonElement);
  trigger.focus();

  press(trigger, "b");
  expect(source.value).toBe("Beta");
  press(trigger, "b");
  expect(source.value).toBe("Bravo");
  expect(root.activeElement).toBe(trigger);
});

test("select value and focus survive a plugin re-render", () => {
  render(
    '<select data-field="city"><option value="berlin">Berlin</option><option value="oslo">Oslo</option></select>',
  );
  const source = one("select", HTMLSelectElement);
  source.value = "oslo";
  const trigger = one(".sb-select-trigger", HTMLButtonElement);
  trigger.focus();
  const snapshot = snapshotFields(root);

  root.innerHTML =
    '<select data-field="city"><option value="berlin">Berlin</option><option value="oslo">Oslo</option></select>';
  restoreFields(root, snapshot);
  syncKit(root);

  expect(one("select", HTMLSelectElement).value).toBe("oslo");
  expect(root.activeElement).toBe(one(".sb-select-trigger", HTMLButtonElement));
});

test("form reset resynchronizes the visible value", async () => {
  render(`
    <form>
      <select><option value="a" selected>Alpha</option><option value="b">Beta</option></select>
    </form>`);
  const source = one("select", HTMLSelectElement);
  const trigger = one(".sb-select-trigger", HTMLButtonElement);
  const beta = root.querySelectorAll<HTMLElement>(".sb-select-option")[1];
  if (beta === undefined) throw new Error("beta missing");
  click(trigger);
  click(beta);
  expect(source.value).toBe("b");

  source.form?.reset();
  await Promise.resolve();
  expect(source.value).toBe("a");
  expect(trigger.textContent).toBe("Alpha");
});
