// @vitest-environment happy-dom
/**
 * Anti-drift between the behaviour layer and what an agent is told about it.
 *
 * A plugin author's LLM knows only what the MCP contract hands it. A hook
 * that works but is undocumented is a capability nobody can use; a hook that
 * is documented but unimplemented is worse — the agent writes markup that
 * silently does nothing. Both directions are checked here.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import contract from "../../../../ui-kit/contract.json";
import { sanitizeHtml } from "../sanitize";
import type { SanitizerDrop } from "../sanitize";

import { KIT_INTERACTIVE } from "./index";

const HERE = join(import.meta.dirname, ".");

interface Hook {
  readonly hook: string;
  readonly what: string;
  readonly example: string;
}

const HOOKS: Hook[] = contract.behaviour.hooks;

/**
 * Attributes the SHELL writes on elements it built itself.
 *
 * They are not author hooks: nobody writes `data-sb-active` into plugin
 * markup, the calendar writes it on its own panel. Documenting them would
 * invite an agent to set them by hand.
 */
const SHELL_OWNED = new Set([
  "data-sb-index",
  "data-sb-source",
  "data-sb-ready",
  "data-sb-active",
  "data-sb-pending",
  "data-sb-opener",
  "data-sb-typeahead",
  "data-sb-typeahead-at",
  // The value-memory scope ShadowHost stamps on its render wrapper.
  "data-sb-scope",
]);

/** Every `data-sb-*` attribute the behaviour modules listen for. */
function implementedHooks(): Set<string> {
  const found = new Set<string>();
  for (const file of readdirSync(HERE)) {
    if (!file.endsWith(".ts") || file.endsWith(".test.ts")) continue;
    const source = readFileSync(join(HERE, file), "utf8");
    for (const match of source.matchAll(/\[(data-sb-[a-z-]+)/g)) {
      if (match[1] !== undefined) found.add(match[1]);
    }
    // `element.dataset.sbFoo` reads the same attribute in camelCase.
    for (const match of source.matchAll(/dataset\.(sb[A-Z][A-Za-z]*)/g)) {
      const camel = match[1];
      if (camel === undefined) continue;
      found.add(`data-${camel.replace(/(?!^)([A-Z])/g, "-$1").toLowerCase()}`);
    }
  }
  return found;
}

/** Hooks named anywhere in the contract's behaviour section. */
function documentedHooks(): Set<string> {
  const found = new Set<string>();
  for (const entry of HOOKS) {
    found.add(entry.hook);
    for (const text of [entry.what, entry.example]) {
      for (const match of text.matchAll(/data-sb-[a-z-]+/g))
        found.add(match[0]);
    }
  }
  return found;
}

test("every documented hook is one a behaviour module implements", () => {
  const implemented = implementedHooks();
  const phantom = [...documentedHooks()].filter(
    (hook) => !implemented.has(hook),
  );
  expect(phantom).toEqual([]);
});

test("every implemented hook is documented in the contract", () => {
  const documented = documentedHooks();
  const undocumented = [...implementedHooks()].filter(
    (hook) => !documented.has(hook) && !SHELL_OWNED.has(hook),
  );
  expect(undocumented).toEqual([]);
});

test("every hook example survives the sanitizer unchanged", () => {
  const drops: SanitizerDrop[] = [];
  for (const entry of HOOKS) {
    const before = drops.length;
    const fragment = sanitizeHtml(
      entry.example,
      (value) => value,
      (drop) => drops.push(drop),
    );
    // Markup that survives the allowlist but renders nothing is just as
    // useless, so the hook itself must still be on the result.
    const host = document.createElement("div");
    host.appendChild(fragment);
    expect(
      host.querySelector(`[${entry.hook}]`),
      `${entry.hook}: the hook attribute is gone after sanitizing`,
    ).not.toBeNull();
    expect(
      drops.slice(before),
      `${entry.hook}: the example loses markup`,
    ).toEqual([]);
  }
});

test("hook markup stops its own click so a flyout stays open", () => {
  // A click that reaches the bar closes the flyout around it. Every hook
  // that reacts to a click has to be in KIT_INTERACTIVE, or using it would
  // dismiss the surface it lives on — the bug native <summary> already hit.
  const clickDriven = [
    "data-sb-copy",
    "data-sb-number-up",
    "data-sb-number-down",
    "data-sb-password-toggle",
    "data-sb-dropdown-toggle",
    "data-sb-context",
    "data-sb-multiselect",
    "data-sb-taginput",
    "data-sb-combobox",
    "data-sb-datepicker",
    "data-sb-daterange",
  ];
  const host = document.createElement("div");
  for (const hook of clickDriven) {
    host.innerHTML = `<div ${hook}></div>`;
    const element = host.firstElementChild;
    expect(element?.matches(KIT_INTERACTIVE), `${hook} is not covered`).toBe(
      true,
    );
  }
});

test("the contract explains that hook state lives in the DOM", () => {
  // Without this an agent builds a sortable table in a plugin that
  // re-renders every second and cannot work out why the order resets.
  expect(contract.behaviour.stateNote).toMatch(/re-render/i);
  expect(contract.behaviour.stateNote).toMatch(/data-field/);
});
