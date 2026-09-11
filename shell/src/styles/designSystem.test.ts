import { expect, test } from "vitest";

import componentTokens from "../../../ui-kit/component-tokens.json";
import kitClasses from "../../../ui-kit/kit-classes.json";
import themeContract from "../../../ui-kit/theme-contract.json";
import theme from "../../../themes/default.json";
import barCss from "./bar.css?raw";
import globalsCss from "./globals.css?raw";
import harvestedCss from "./kit-components.css?raw";
import overlayCss from "./overlay.css?raw";
import settingsWindowCss from "./settings-window.css?raw";
import settingsCss from "./settings.css?raw";
import settingsFontsCss from "./settings-fonts.css?raw";
import kitCss from "./ui-kit.css?raw";

const allCss = [
  globalsCss,
  barCss,
  overlayCss,
  settingsWindowCss,
  settingsCss,
  settingsFontsCss,
  kitCss,
  harvestedCss,
].join("\n");

function rule(css: string, selector: string): string {
  const uncommented = css.replace(/\/\*.*?\*\//gs, "");
  const bodies: string[] = [];
  for (const match of uncommented.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (match[1]?.split(",").some((part) => part.trim() === selector)) {
      bodies.push(match[2] ?? "");
    }
  }
  return bodies.join("\n");
}

test("field, joined-control, and custom-select contracts have one owner", () => {
  expect(rule(kitCss, ".sb-field")).toContain("flex-direction: row");
  expect(rule(kitCss, ".sb-field-stack")).toContain("flex-direction: column");
  expect(rule(kitCss, ".sb-field__hint")).toContain("margin: 0");
  expect(rule(kitCss, ".sb-btn-group")).toContain("gap: 0");
  expect(rule(kitCss, ".sb-input-group")).toContain("gap: 0");
  expect(rule(kitCss, ".sb-select-list")).toContain(
    "z-index: var(--sb-z-plugin-menu)",
  );
  expect(rule(kitCss, ".sb-select-list[data-top-layer]")).toContain(
    "position: fixed",
  );
  expect(rule(kitCss, ".sb-select-source")).toContain("clip-path: inset(50%)");
  expect(rule(kitCss, ".sb-select[data-sb-native]")).toContain(
    "appearance: auto",
  );
  for (const selector of [
    "stack",
    "grid",
    "card",
    "carousel",
    "btn-group",
    "field-stack",
    "input-group",
  ]) {
    expect(harvestedCss, selector).not.toMatch(
      new RegExp(`^\\.sb-${selector}\\s*\\{`, "m"),
    );
  }
});

test("scrollable lists preserve their scroll boundary", () => {
  expect(rule(kitCss, ".sb-list")).toContain("overflow: visible");
  expect(rule(kitCss, ".sb-list.sb-scroll")).toContain("overflow-y: auto");
});

test("the settings frame cannot become a programmatic scroll container", () => {
  expect(rule(settingsWindowCss, ".settings-panel")).toContain(
    "overflow: clip",
  );
  expect(rule(settingsWindowCss, ".settings-group-content")).toContain(
    "overflow-y: auto",
  );
});

test("a narrow split zone scrolls whole tiles instead of crushing them", () => {
  // Scroll-container clipping is opt-in: only an actually overflowing zone
  // (data-zone-overflowing, useZoneScroll) clips — a fitting zone stays
  // overflow-visible so hover/magnify effects can grow past the bar edge.
  expect(rule(barCss, ".zone-scroll")).toContain("overflow-x: visible");
  expect(rule(barCss, ".zone-scroll[data-zone-overflowing]")).toContain(
    "overflow-x: auto",
  );
  expect(rule(barCss, ".zone-scroll > *")).toContain("flex-shrink: 0");
});

test("every bar hover style requires native pointer presence", () => {
  const hoverSelectors = [...barCss.matchAll(/([^{}]*:hover[^{}]*)\{/g)];
  expect(hoverSelectors.length).toBeGreaterThan(0);
  for (const selector of hoverSelectors) {
    expect(selector[1]).toContain(":root:not([data-sb-pointer-outside])");
  }
});

test("sb-center centers text and children in flex or grid cards", () => {
  expect(rule(kitCss, ".sb-center")).toContain("text-align: center");
  expect(rule(kitCss, ".sb-center")).toContain("align-items: center");
  expect(rule(kitCss, ".sb-center")).toContain("justify-items: center");
});

test("intentional unlayered modifier implementations stay public", () => {
  const documented = new Set(kitClasses.classes.map(({ name }) => name));
  for (const name of [
    "sb-btn--primary",
    "sb-btn--ghost",
    "sb-btn--danger",
    "sb-card--accent",
    "sb-card--interactive",
    "sb-mesh",
  ]) {
    expect(documented.has(name), name).toBe(true);
  }
});

test("carousel drag state matches the shell-owned pointer enhancer", () => {
  expect(rule(kitCss, ".sb-carousel")).toContain("touch-action: pan-y");
  expect(kitCss).toContain(".sb-carousel:is(.is-dragging, [data-dragging])");
  expect(kitCss).toMatch(
    /\.sb-carousel:is\(\.is-dragging, \[data-dragging\]\)\s*\{[^}]*scroll-behavior: auto/,
  );
});

test("public rhythm and radius knobs reach their semantic owners", () => {
  for (const selector of [
    ".sb-stack",
    ".sb-inline",
    ".sb-field",
    ".sb-grid",
    ".sb-cols-2",
    ".sb-card-grid",
    ".sb-cluster",
  ]) {
    expect(rule(kitCss, selector), selector).toContain("--sb-gap");
  }
  expect(rule(kitCss, ".sb-field-stack")).toContain("--sb-field-gap");
  expect(rule(kitCss, ".sb-card")).toContain("--sb-card-gap");
  expect(rule(kitCss, ".sb-btn")).toContain("--sb-btn-radius");
  expect(rule(kitCss, ".sb-input")).toContain("--sb-input-radius");
  expect(
    rule(kitCss, ".sb-input-group > .sb-select-shell > .sb-select-trigger"),
  ).toContain("border-radius: 0");
  expect(rule(kitCss, ".sb-card > .sb-card__media:first-child")).toContain(
    "border-start-start-radius: inherit",
  );
});

test("component design values stay behind documented tokens", () => {
  expect(rule(kitCss, ".sb-btn")).toMatch(
    /font-weight:\s*var\(--sb-weight-medium/,
  );
  expect(rule(kitCss, ".sb-btn")).toMatch(
    /line-height:\s*var\(--sb-leading-control/,
  );
  expect(rule(kitCss, ".sb-btn")).toMatch(
    /min-height:\s*var\(--sb-control-min-height/,
  );
  for (const property of ["gap", "width", "height", "padding"]) {
    expect(rule(settingsCss, ".settings-swatch")).toMatch(
      new RegExp(`${property}:\\s*var\\(--sb-settings-swatch-`),
    );
  }
  expect(rule(overlayCss, ".overlay-tooltip")).toContain(
    "padding: var(--sb-tooltip-padding",
  );
  expect(rule(overlayCss, ".overlay-tooltip")).toContain(
    "line-height: var(--sb-leading-help",
  );

  const generated = harvestedCss.replace(/\/\*.*?\*\//gs, "");
  expect(generated).not.toMatch(
    /(?:gap|padding|margin|border-radius|box-shadow):\s*[+-]?(?:\d*\.)?\d+(?:px|rem|em|%)/,
  );
  expect(generated).not.toMatch(
    /(?:font-weight|line-height|letter-spacing):\s*[+-]?(?:\d*\.)?\d/,
  );
});

test("bar and flyout opacity are independent and overlays stay opaque", () => {
  for (const selector of [".surface-bar", ".surface-tile", ".surface-inner"]) {
    expect(rule(barCss, selector), selector).toContain("var(--sb-bar-opacity)");
  }
  expect(rule(barCss, ".surface-flyout")).toContain("var(--sb-flyout-opacity)");
  expect(rule(barCss, ".surface-flyout")).toContain(
    "--sb-bar-opacity: var(--sb-flyout-opacity)",
  );
  expect(rule(kitCss, ".sb-modal")).toContain(
    "background: var(--sb-overlay-bg)",
  );
  expect(rule(kitCss, ".sb-drawer")).toContain(
    "background: var(--sb-overlay-bg)",
  );
  expect(rule(settingsWindowCss, ".settings-window")).not.toContain(
    "--sb-bar-opacity",
  );
  expect(`${kitCss}\n${harvestedCss}`).not.toMatch(
    /background(?:-color)?:\s*var\(--sb-(?:inner|tile|tile-hover)-bg\)/,
  );
  expect(harvestedCss).not.toContain(
    "background: light-dark(var(--sb-inner-bg)",
  );
  expect(rule(harvestedCss, ".sb-skeleton")).toContain("--sb-bar-opacity");
});

test("code panels stay compact inside narrow plugin flyouts", () => {
  expect(rule(harvestedCss, ".sb-code__body")).toContain(
    "font-size: var(--sb-fs-s)",
  );
  expect(rule(harvestedCss, ".sb-code__body")).toContain(
    "line-height: var(--sb-leading-control",
  );
});

test("settings keep readable type and smooth inaccessible-state reveals", () => {
  const panel = rule(settingsWindowCss, ".settings-panel");
  expect(panel).toContain("--sb-fs-xs: max(12px, 0.75rem)");
  expect(panel).toContain("--sb-fs-m: max(14px, 0.875rem)");
  expect(panel).toContain("--sb-fs-xl: max(18px, 1.125rem)");
  expect(rule(settingsCss, ".settings-help")).toContain(
    "color: var(--sb-text-dim)",
  );
  expect(rule(settingsCss, ".settings-row-label")).toContain(
    "font-size: var(--sb-fs-m)",
  );
  expect(rule(settingsCss, ".settings-help")).toContain(
    "font-size: var(--sb-fs-s)",
  );
  expect(rule(settingsCss, ".settings-reveal")).toContain("opacity: 0");
  expect(rule(settingsCss, ".settings-reveal.sb-active")).toContain(
    "opacity: 1",
  );
  expect(settingsCss).toMatch(
    /@media \(prefers-reduced-motion: reduce\)[\s\S]*\.settings-reveal[\s\S]*transition: none/,
  );
});

test("chevrons and z-indexes are color- and layer-token driven", () => {
  expect(allCss).not.toMatch(/background(?:-image)?:\s*var\(--sb-chevron\)/);
  expect(allCss).toContain("mask: var(--sb-chevron)");
  expect(allCss).toContain("background-color: currentcolor");
  expect(allCss).not.toMatch(/z-index:\s*-?\d/);
  expect(globalsCss).toContain("--sb-z-tooltip: 140");
});

test("charts expose axes, grid, empty, loading, and forced-color states", () => {
  expect(kitCss).toContain(".sb-chart-bars[data-grid]::before");
  expect(kitCss).toContain("[data-axis]");
  expect(kitCss).toContain(".is-empty");
  expect(kitCss).toContain(".is-loading");
  expect(kitCss).toContain("@media (forced-colors: active)");
  expect(rule(kitCss, ".sb-chart-rows__bar")).toContain("0px");
  expect(kitCss).toContain(".sb-chart-rows__bar::before");
  expect(kitCss).toContain("background: CanvasText");
  expect(harvestedCss).not.toMatch(/^\s*--sb-chart-[1-4]:/m);
  expect(themeContract.baseTokens["--sb-chart-1"].type).toBe("color");
});

test("menus, forced-color focus, and infinite motion use shared contracts", () => {
  expect(rule(kitCss, ".sb-dropdown__menu")).toContain(
    "background: var(--sb-menu-bg)",
  );
  expect(kitCss).toContain("background: var(--sb-menu-hover-bg)");
  expect(kitCss).toContain("color: var(--sb-menu-danger)");
  expect(kitCss).toContain(".sb-card--laser::before");
  expect(kitCss).toContain(".sb-progress:indeterminate");
  expect(kitCss).toMatch(
    /@media \(prefers-reduced-motion: reduce\)[\s\S]*\.sb-progress:not\(progress\) > \*[\s\S]*transition: none/,
  );
  expect(kitCss).toContain(".sb-chat__typing > span");
  expect(kitCss).toContain("outline: 2px solid Highlight");
  expect(overlayCss).toContain(".overlay-menu-item:focus-visible");
});

test("the generated non-theme token census covers CSS and is disjoint", () => {
  const base = new Set(
    Object.keys(theme).filter((name) => name.startsWith("--sb-")),
  );
  const uncommented = allCss.replace(/\/\*.*?\*\//gs, "");
  const referenced = new Set(uncommented.match(/--sb-[a-z0-9-]+/g) ?? []);
  const publicNames = Object.keys(componentTokens.publicComponentTokens);
  const internalNames = Object.keys(componentTokens.internalTokens);
  const census = new Set([...publicNames, ...internalNames]);
  const expected = [...referenced].filter((name) => !base.has(name)).sort();

  expect(publicNames.filter((name) => internalNames.includes(name))).toEqual(
    [],
  );
  expect(expected.filter((name) => !census.has(name))).toEqual([]);
  for (const token of [
    ...Object.values(componentTokens.publicComponentTokens),
    ...Object.values(componentTokens.internalTokens),
  ]) {
    expect(token.allowed).toBeDefined();
    expect(token.scope).toMatch(/^(?:component|runtime)$/);
    expect(typeof token.themeable).toBe("boolean");
    expect(token.group).toMatch(/^(?:component|runtime)\./);
    expect(token.consumers.length).toBeGreaterThan(0);
  }
  for (const token of Object.values(componentTokens.internalTokens)) {
    expect(token.nonThemeable).toBe(true);
  }
});

test("the generated census includes runtime and base-token consumers", () => {
  expect(componentTokens.internalTokens["--sb-probe"].consumers).toContain(
    "shell/src/ipc/capabilities.ts",
  );
  expect(componentTokens.internalTokens["--sb-context-x"].consumers).toContain(
    "shell/src/plugins/behaviour/menus.ts",
  );
  expect(
    componentTokens.internalTokens["--sb-shortcut-icon-size"].consumers,
  ).toContain("shell/src/components/bar/ShortcutZone.tsx");
  expect(themeContract.baseTokens["--sb-color-scheme"].consumers).toEqual(
    expect.arrayContaining([
      "shell/src/styles/globals.css",
      "shell/src/styles/overlay.css",
      "shell/src/styles/settings-window.css",
    ]),
  );
  expect(
    themeContract.baseTokens["--sb-chart-1"].cssConsumers.map(
      ({ selector }) => selector,
    ),
  ).toEqual(
    expect.arrayContaining([
      ".sb-chart-donut::before",
      ".sb-chart-legend__item::before",
    ]),
  );
});

test("contextual component defaults retain exact selector provenance", () => {
  const gap = componentTokens.publicComponentTokens["--sb-gap"];
  expect(gap.default).toBeNull();
  const fieldDefault = gap.contextualDefaults.find(
    ({ value }) => value === "var(--sb-space-xs)",
  );
  expect(fieldDefault?.consumers).toContainEqual({
    file: "shell/src/styles/ui-kit.css",
    selector: ".sb-field",
  });
  expect(gap.cssConsumers.map(({ selector }) => selector)).toContain(
    ".sb-cols-2, .sb-cols-3, .sb-cols-4, .sb-card-grid",
  );
  expect(componentTokens.internalTokens["--sb-chart-max"].themeable).toBe(
    false,
  );
});

test("known component aliases keep their semantic token types", () => {
  const publicTokens = componentTokens.publicComponentTokens;
  const typeOf = (name: keyof typeof publicTokens) => publicTokens[name].type;
  for (const name of [
    "--sb-copy-icon",
    "--sb-rating-star",
    "--sb-stepper-check",
  ] as const) {
    expect(typeOf(name), name).toBe("image");
  }
  for (const name of [
    "--sb-datepicker-radius",
    "--sb-menu-radius",
    "--sb-spotlight-blur",
  ] as const) {
    expect(typeOf(name), name).toBe("dimension");
  }
  expect(typeOf("--sb-tree-line")).toBe("color");
});
