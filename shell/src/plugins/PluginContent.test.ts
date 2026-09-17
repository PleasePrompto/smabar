// @vitest-environment happy-dom
import { expect, test } from "vitest";

import { brandingStyle, hostBranding } from "./branding";
import { findActionRange, rangeActionIntent } from "./fields";
import {
  collectFieldValues,
  handlesItsOwnClick,
  restoreFields,
  snapshotFields,
  submitIntent,
} from "./PluginContent";
import { flyoutContentFor } from "../components/overlay/model";

function fragmentOf(html: string): DocumentFragment {
  const template = document.createElement("template");
  template.innerHTML = html;
  return template.content;
}

test("collects input values keyed by data-field", () => {
  const root = fragmentOf(
    '<div><input data-field="nummer"><input data-field="plz"></div>',
  );
  const inputs = [...root.querySelectorAll("input")];
  for (const [index, value] of ["156510313639", "48155"].entries()) {
    const input = inputs.at(index);
    if (input) input.value = value;
  }
  expect(collectFieldValues(root)).toEqual({
    nummer: "156510313639",
    plz: "48155",
  });
});

test("returns undefined without any data-field inputs", () => {
  expect(collectFieldValues(fragmentOf("<div><input></div>"))).toBeUndefined();
});

function setValue(root: ParentNode, field: string, value: string): void {
  const input = root.querySelector<HTMLInputElement>(
    `input[data-field="${field}"]`,
  );
  if (input) input.value = value;
}

test("snapshot and restore carry typed values across a re-render", () => {
  const before = fragmentOf(
    '<div><input data-field="nummer"><input data-field="plz"></div>',
  );
  setValue(before, "nummer", "156510313639");
  setValue(before, "plz", "48155");

  // The plugin re-rendered the same form with empty inputs.
  const after = fragmentOf(
    '<div><input data-field="nummer"><input data-field="plz"></div>',
  );
  restoreFields(after, snapshotFields(before));
  expect(collectFieldValues(after)).toEqual({
    nummer: "156510313639",
    plz: "48155",
  });
});

test("plugin-prefilled values win over restored ones", () => {
  const before = fragmentOf('<div><input data-field="city"></div>');
  setValue(before, "city", "typed");

  const after = fragmentOf(
    '<div><input data-field="city" value="from-plugin"></div>',
  );
  restoreFields(after, snapshotFields(before));
  expect(collectFieldValues(after)).toEqual({ city: "from-plugin" });
});

test("UA defaults do not overwrite restored range and color values", () => {
  const before = fragmentOf(
    '<input type="range" data-field="level"><input type="color" data-field="tint">',
  );
  setValue(before, "level", "83");
  setValue(before, "tint", "#abcdef");
  const after = fragmentOf(
    '<input type="range" data-field="level"><input type="color" data-field="tint">',
  );

  restoreFields(after, snapshotFields(before));
  expect(collectFieldValues(after)).toEqual({
    level: "83",
    tint: "#abcdef",
  });
});

test("an explicit empty input value wins over restored text", () => {
  const before = fragmentOf('<input data-field="query">');
  setValue(before, "query", "typed");
  const after = fragmentOf('<input data-field="query" value="">');

  restoreFields(after, snapshotFields(before));
  expect(collectFieldValues(after)).toEqual({ query: "" });
});

test("restore tolerates fields missing from the new markup", () => {
  const before = fragmentOf(
    '<div><input data-field="keep"><input data-field="gone"></div>',
  );
  setValue(before, "keep", "still-here");
  setValue(before, "gone", "dropped");

  const after = fragmentOf('<div><input data-field="keep"></div>');
  restoreFields(after, snapshotFields(before));
  expect(collectFieldValues(after)).toEqual({ keep: "still-here" });
});

test("checkboxes travel as checked state, not value", () => {
  const root = fragmentOf(
    '<div><input type="checkbox" data-field="dark">' +
      '<input data-field="city"></div>',
  );
  const box = root.querySelector<HTMLInputElement>('input[data-field="dark"]');
  if (box) box.checked = true;
  setValue(root, "city", "Berlin");
  expect(collectFieldValues(root)).toEqual({ dark: "true", city: "Berlin" });

  // Snapshot + restore carry the toggle across a re-render …
  const after = fragmentOf(
    '<div><input type="checkbox" data-field="dark"></div>',
  );
  restoreFields(after, snapshotFields(root));
  expect(collectFieldValues(after)).toEqual({ dark: "true" });

  // … but an explicit plugin-set checked attribute wins.
  const preset = fragmentOf(
    '<div><input type="checkbox" data-field="dark" checked></div>',
  );
  const empty = fragmentOf(
    '<div><input type="checkbox" data-field="dark"></div>',
  );
  restoreFields(preset, snapshotFields(empty));
  expect(collectFieldValues(preset)).toEqual({ dark: "true" });
});

test("selects, textareas, and the checked radio travel as field values", () => {
  const root = fragmentOf(`
    <textarea data-field="note">saved note</textarea>
    <select data-field="city"><option value="berlin">Berlin</option><option value="oslo" selected>Oslo</option></select>
    <input type="radio" data-field="unit" value="c">
    <input type="radio" data-field="unit" value="f" checked>`);
  expect(collectFieldValues(root)).toEqual({
    note: "saved note",
    city: "oslo",
    unit: "f",
  });
});

test("select, textarea, and radio state survives an empty re-render", () => {
  const before = fragmentOf(`
    <textarea data-field="note"></textarea>
    <select data-field="city"><option value="berlin">Berlin</option><option value="oslo">Oslo</option></select>
    <input type="radio" data-field="unit" value="c">
    <input type="radio" data-field="unit" value="f">`);
  const note = before.querySelector<HTMLTextAreaElement>("textarea");
  const city = before.querySelector<HTMLSelectElement>("select");
  const fahrenheit = before.querySelector<HTMLInputElement>('input[value="f"]');
  if (note) note.value = "typed";
  if (city) city.value = "oslo";
  if (fahrenheit) fahrenheit.checked = true;

  const after = fragmentOf(`
    <textarea data-field="note"></textarea>
    <select data-field="city"><option value="berlin">Berlin</option><option value="oslo">Oslo</option></select>
    <input type="radio" data-field="unit" value="c">
    <input type="radio" data-field="unit" value="f">`);
  restoreFields(after, snapshotFields(before));
  expect(collectFieldValues(after)).toEqual({
    note: "typed",
    city: "oslo",
    unit: "f",
  });
});

test("plugin-authored select and radio choices win over restored state", () => {
  const before = fragmentOf(`
    <select data-field="city"><option value="berlin">Berlin</option><option value="oslo" selected>Oslo</option></select>
    <input type="radio" data-field="unit" value="c">
    <input type="radio" data-field="unit" value="f" checked>`);
  const after = fragmentOf(`
    <select data-field="city"><option value="berlin" selected>Berlin</option><option value="oslo">Oslo</option></select>
    <input type="radio" data-field="unit" value="c" checked>
    <input type="radio" data-field="unit" value="f">`);
  restoreFields(after, snapshotFields(before));
  expect(collectFieldValues(after)).toEqual({ city: "berlin", unit: "c" });
});

test("hover preview shows hover content, a pinned flyout the click content", () => {
  // Both pushed: the mode decides.
  expect(flyoutContentFor("peek", "<p>hover</p>", "<p>click</p>")).toBe(
    "<p>hover</p>",
  );
  expect(flyoutContentFor("pinned", "<p>hover</p>", "<p>click</p>")).toBe(
    "<p>click</p>",
  );
  // Only one pushed: both modes fall back to it.
  expect(flyoutContentFor("peek", undefined, "<p>click</p>")).toBe(
    "<p>click</p>",
  );
  expect(flyoutContentFor("pinned", "<p>hover</p>", undefined)).toBe(
    "<p>hover</p>",
  );
  // Nothing pushed yet.
  expect(flyoutContentFor("peek", undefined, undefined)).toBeUndefined();
  expect(flyoutContentFor(null, undefined, undefined)).toBeUndefined();
});

test("hostBranding applies a tile's branding only in plugin accent mode", () => {
  const style = { "--sb-accent": "#336699" };
  expect(hostBranding("plugin", style)).toBe(style);
  expect(hostBranding("theme", style)).toBeUndefined();
  expect(hostBranding("plugin", undefined)).toBeUndefined();
});

test("brandingStyle builds host token overrides only when accent is set", () => {
  expect(brandingStyle({ id: "w", name: "W" })).toBeUndefined();
  expect(
    brandingStyle({ id: "w", name: "W", accentFg: "#111" }),
  ).toBeUndefined();
  // No accentFg on a pale brand colour: the kit derives every text step on
  // accent surfaces from --sb-on-accent, so leaving it at the theme's white
  // painted white on yellow. It is now derived instead.
  expect(brandingStyle({ id: "w", name: "W", accent: "#FFCC00" })).toEqual({
    "--sb-accent": "#FFCC00",
    "--sb-accent-2": "#FFCC00",
    "--sb-accent-gradient":
      "linear-gradient(135deg, var(--sb-accent), var(--sb-accent-2))",
    "--sb-accent-glow":
      "0 2px 12px -2px color-mix(in srgb, var(--sb-accent) 35%, transparent)",
    "--sb-on-accent": "#09060f",
  });
  // A dark accent derives white.
  expect(brandingStyle({ id: "w", name: "W", accent: "#1e3a8a" })).toEqual({
    "--sb-accent": "#1e3a8a",
    "--sb-accent-2": "#1e3a8a",
    "--sb-accent-gradient":
      "linear-gradient(135deg, var(--sb-accent), var(--sb-accent-2))",
    "--sb-accent-glow":
      "0 2px 12px -2px color-mix(in srgb, var(--sb-accent) 35%, transparent)",
    "--sb-on-accent": "#ffffff",
  });
  // A declared accentFg that works is respected verbatim.
  expect(
    brandingStyle({
      id: "w",
      name: "W",
      accent: "#FFCC00",
      accent2: "#D40511",
      accentFg: "#1a1a1a",
    }),
  ).toEqual({
    "--sb-accent": "#FFCC00",
    "--sb-accent-2": "#D40511",
    "--sb-on-accent": "#1a1a1a",
    "--sb-accent-gradient":
      "linear-gradient(135deg, var(--sb-accent), var(--sb-accent-2))",
    "--sb-accent-glow":
      "0 2px 12px -2px color-mix(in srgb, var(--sb-accent) 35%, transparent)",
  });
  // A declared accentFg that does NOT work is corrected.
  expect(
    brandingStyle({
      id: "w",
      name: "W",
      accent: "#FFCC00",
      accentFg: "#ffffff",
    })?.["--sb-on-accent"],
  ).toBe("#09060f");
});

test("focus and cursor return to the focused field", () => {
  document.body.innerHTML =
    '<div><input data-field="nummer"><input data-field="plz"></div>';
  setValue(document, "nummer", "156510313639");
  const plz = document.querySelector<HTMLInputElement>(
    'input[data-field="plz"]',
  );
  plz?.focus();

  const snapshot = snapshotFields(document);
  expect(snapshot.focused).toBe("plz");

  document.body.innerHTML =
    '<div><input data-field="nummer"><input data-field="plz" value="48155"></div>';
  restoreFields(document, snapshot);

  const restored = document.querySelector<HTMLInputElement>(
    'input[data-field="plz"]',
  );
  expect(document.activeElement).toBe(restored);
  expect(restored?.selectionStart).toBe("48155".length);
  document.body.innerHTML = "";
});

test("pressing Enter in a form sends the form's declared action", () => {
  // A <form> may exist for labels and native validation, but a real submit
  // would navigate the webview away from the bar — so Enter becomes the same
  // action a click on the submit button sends.
  const root = fragmentOf(
    '<form><input data-field="zone">' +
      '<button data-action="add">Add</button></form>',
  );
  const form = root.querySelector("form");
  expect(submitIntent(form)).toEqual({ action: "add" });
});

test("an explicit data-value on the submit element wins over the fields", () => {
  const root = fragmentOf(
    '<form><button data-action="pick" data-value="berlin">Go</button></form>',
  );
  expect(submitIntent(root.querySelector("form"))).toEqual({
    action: "pick",
    value: "berlin",
  });
});

test("a form declaring no action sends nothing at all", () => {
  const root = fragmentOf('<form><input data-field="a"></form>');
  expect(submitIntent(root.querySelector("form"))).toBeNull();
  expect(submitIntent(null)).toBeNull();
  expect(submitIntent(document.createElement("div"))).toBeNull();
});

test("a range data-action sends its committed live value", () => {
  const root = fragmentOf(
    '<input type="range" min="0" max="100" value="37" data-action="setVolume">',
  );
  const range = root.querySelector("input");
  expect(rangeActionIntent(range)).toEqual({
    action: "setVolume",
    value: "37",
    occurrence: 0,
  });
  expect(rangeActionIntent(document.createElement("button"))).toBeNull();
});

test("duplicate range actions retain their own occurrence", () => {
  const markup =
    '<input type="range" data-action="setVolume" data-field="volume" value="20">' +
    '<input type="range" data-action="setVolume" data-field="volume" value="80">';
  const before = fragmentOf(markup);
  const second = before.querySelectorAll<HTMLInputElement>("input")[1];
  const intent = rangeActionIntent(second ?? null);
  if (intent === null) throw new Error("range intent missing");
  expect(intent.occurrence).toBe(1);
  expect(findActionRange(fragmentOf(markup), intent)?.value).toBe("80");
});

test("natively interactive elements keep their click to themselves", () => {
  // A click that bubbles out of a flyout closes it. Before this, opening a
  // dialog from inside a flyout shut the flyout instead.
  const root = fragmentOf(
    '<div><button commandfor="d" command="show-modal">a</button>' +
      '<button popovertarget="m">b</button><summary>c</summary>' +
      "<select></select><textarea></textarea><label>d</label>" +
      '<button data-action="go">e</button><span>f</span></div>',
  );
  const own = [...root.querySelectorAll<HTMLElement>("*")].filter(
    handlesItsOwnClick,
  );
  expect(own.map((el) => el.localName)).toEqual([
    "button",
    "button",
    "summary",
    "select",
    "textarea",
    "label",
  ]);
  // A plain data-action button still goes through the plugin action path.
  const action = root.querySelector<HTMLElement>("[data-action]");
  expect(action && handlesItsOwnClick(action)).toBe(false);
});
