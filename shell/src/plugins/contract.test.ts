// @vitest-environment happy-dom
// @vitest-environment-options { "settings": { "navigation": { "disableChildFrameNavigation": true } } }
/**
 * Anti-drift tests: ui-kit/contract.json (served to agents via the MCP
 * ui_kit tool) must mechanically match the shell implementation — the
 * sanitizer's allowlists, the icon registry, the kit stylesheet's token
 * usage, and the copy-paste snippets.
 */
import { expect, test } from "vitest";

import contract from "../../../ui-kit/contract.json";
import defaultTheme from "../../../themes/default.json";
import componentTokens from "../../../ui-kit/component-tokens.json";
import {
  CONTEXT_ITEMS_ATTR,
  CONTEXT_MENU_MAX_ACTION_CHARS,
  CONTEXT_MENU_MAX_ITEMS,
  CONTEXT_MENU_MAX_LABEL_CHARS,
  CONTEXT_MENU_MAX_SPEC_CHARS,
  CONTEXT_MENU_MAX_VALUE_CHARS,
  parseContextItems,
} from "../components/overlay/pluginMenu";
import { TOOLTIP_ATTR, TOOLTIP_DELAY_MS } from "../components/overlay/Tooltip";
import barCss from "../styles/bar.css?inline";
import overlayCss from "../styles/overlay.css?inline";
import harvestedCss from "../styles/kit-components.css?inline";
import kitCss from "../styles/ui-kit.css?inline";
import { ICONS } from "./icons";
import {
  POPUP_TTL_MAX_MS,
  POPUP_TTL_MIN_MS,
  POPUP_VISIBLE_MAX,
} from "./popupQueue";
import {
  ALLOWED_ATTRS,
  ALLOWED_TAGS,
  AUDIO_ATTRS,
  COMMAND_VALUES,
  CONTROL_ATTRS,
  DATA_ATTR_PATTERN,
  DROPPED_TAGS,
  EMBED_ALLOW,
  EMBED_REFERRER_POLICY,
  EMBED_SANDBOX,
  INPUT_TYPES,
  NATIVE_INTERACTION_ATTRS,
  SOURCE_ATTRS,
  SVG_TAGS,
  TRACK_ATTRS,
  TRACK_KINDS,
  VIDEO_ATTRS,
  sanitizeHtml,
} from "./sanitize";

const sorted = (values: Iterable<string>): string[] => [...values].sort();

test("sanitizer contract matches sanitize.ts in both directions", () => {
  expect(sorted(contract.sanitizer.allowedTags)).toEqual(sorted(ALLOWED_TAGS));
  expect(sorted(contract.sanitizer.droppedTags)).toEqual(sorted(DROPPED_TAGS));
  expect(sorted(contract.sanitizer.allowedAttrs)).toEqual(
    sorted(ALLOWED_ATTRS),
  );
  expect(sorted(contract.sanitizer.inputTypes)).toEqual(sorted(INPUT_TYPES));
  expect(sorted(contract.sanitizer.inputAttrs)).toEqual(sorted(CONTROL_ATTRS));
  expect(sorted(contract.media.videoAttrs)).toEqual(sorted(VIDEO_ATTRS));
  expect(sorted(contract.media.audioAttrs)).toEqual(sorted(AUDIO_ATTRS));
  expect(sorted(contract.media.sourceAttrs)).toEqual(sorted(SOURCE_ATTRS));
  expect(sorted(contract.media.trackAttrs)).toEqual(sorted(TRACK_ATTRS));
  expect(sorted(contract.media.trackKinds)).toEqual(sorted(TRACK_KINDS));
  expect(contract.media.embedAllow).toBe(EMBED_ALLOW);
  expect(contract.media.embedSandbox).toBe(EMBED_SANDBOX);
  expect(contract.media.embedReferrerPolicy).toBe(EMBED_REFERRER_POLICY);
  expect(contract.sanitizer.dataAttrPattern).toBe(DATA_ATTR_PATTERN.source);
  // The native-interaction lists carry the whole no-JavaScript story; the
  // guide and the snippets quote them, so they must not drift either.
  expect(sorted(contract.sanitizer.commandValues)).toEqual(
    sorted(COMMAND_VALUES),
  );
  expect(sorted(contract.sanitizer.nativeInteractionAttrs)).toEqual(
    sorted(NATIVE_INTERACTION_ATTRS),
  );
  expect(sorted(contract.sanitizer.svgTags)).toEqual(sorted(SVG_TAGS));
});

test("icon contract matches the icons.ts registry in both directions", () => {
  expect(sorted(contract.icons)).toEqual(sorted(Object.keys(ICONS)));
});

test("every registry entry carries real icon nodes", () => {
  // Guards against lucide's deprecated alias modules, which re-export the
  // component but NOT __iconNode (the import then resolves to undefined).
  for (const [name, node] of Object.entries(ICONS)) {
    expect(Array.isArray(node), `${name} has no icon node`).toBe(true);
    expect(node.length, `${name} is empty`).toBeGreaterThan(0);
  }
});

/** Both sheets reach a plugin shadow root, so both satisfy the contract. */
const allKitCss = `${harvestedCss}\n${kitCss}`;

test("design rules teach stable tile sizing without padding the tile out", () => {
  const rules = contract.designRules.join(" ");
  expect(rules).toContain("sb-mono");
  expect(rules).toContain("data-marquee");
  expect(rules).toContain("data-rotator");
  // The old advice ("set an inline min-width") is what made pills wider than
  // their content. Tabular digits and the rotator already keep tiles steady.
  expect(rules).not.toContain("min-width");
  const scenarios = contract.tileSizing.scenarios;
  expect(scenarios.length).toBeGreaterThanOrEqual(4);
  for (const scenario of scenarios) {
    expect(scenario.content.length).toBeGreaterThan(0);
    expect(scenario.do.length).toBeGreaterThan(0);
  }
});

test("kit tiles hug and center their content", () => {
  const tile = /\.sb-tile \{([^}]*)\}/.exec(kitCss)?.[1] ?? "";
  expect(tile).toContain("justify-content: center");
  expect(tile).not.toContain("min-width");
  // A tile rotator's views must center too, or short views hug the left.
  expect(kitCss).toContain(".sb-tile .sb-rotator");
});

test("accent surfaces derive their text steps from --sb-on-accent", () => {
  // The muted tokens are tuned for the dark surfaces; rules like
  // `:where(.sb-root) small { color: var(--sb-text-muted) }` match the element
  // DIRECTLY and beat the color inherited from .sb-hero, so the tokens
  // themselves have to be shadowed on accent surfaces.
  // Anchored at a line start: the spacing rules list .sb-hero inside an
  // indented :is() group long before the surface rule itself.
  const block = /^\.sb-hero,[^{]*\{([^}]*)\}/m.exec(kitCss)?.[1] ?? "";
  for (const token of [
    "--sb-text:",
    "--sb-text-dim:",
    "--sb-text-muted:",
    "--sb-text-faint:",
  ]) {
    expect(block).toContain(token);
  }
  expect(block).toContain("var(--sb-on-accent)");
  // Shadowing --sb-accent would make .sb-btn-primary invisible in a hero.
  expect(block).not.toContain("--sb-accent:");
  expect(kitCss).toContain(".sb-hero a");
});

test("contract documents inline branding instead of background overrides", () => {
  expect(contract.branding.inline).toContain("--sb-on-accent");
  expect(contract.branding.inline).toContain("sb-hero");
  expect(contract.snippets.heroTint).toContain("--sb-accent-gradient");
});

test("rotator convention matches its enhancer selectors", () => {
  expect(contract.conventions.rotator.attribute).toBe("data-rotator");
  expect(contract.conventions.rotator.usage).toContain("4000");
  expect(contract.conventions.rotator.usage).toContain("1500");
  expect(kitCss).toContain(".sb-rotator");
  expect(kitCss).toContain(".sb-rotator-active");
  expect(kitCss).toContain(".sb-rotator-leaving");
  const markup = sanitizeHtml(
    '<div data-rotator="up" data-rotator-interval="4000"><span>a</span><span>b</span></div>',
  );
  const element = markup.querySelector("div");
  expect(element?.getAttribute("data-rotator")).toBe("up");
  expect(element?.getAttribute("data-rotator-interval")).toBe("4000");
});

test("marquee and badge conventions match their enhancer selectors", () => {
  expect(contract.conventions.marquee.attribute).toBe("data-marquee");
  expect(contract.conventions.badge.attribute).toBe("data-badge");
  expect(kitCss).toContain("[data-marquee-overflow]");
  expect(kitCss).toContain(".sb-marquee-content");
  expect(kitCss).toContain(".sb-data-badge");
  const markup = sanitizeHtml(
    '<span data-marquee data-badge="">Long status</span>',
  );
  const element = markup.querySelector("span");
  expect(element?.hasAttribute("data-marquee")).toBe(true);
  expect(element?.getAttribute("data-badge")).toBe("");
});

test("carousel and tabs conventions match their enhancer selectors", () => {
  expect(contract.conventions.carousel.attribute).toBe("data-carousel");
  expect(contract.conventions.carousel.usage).toContain(
    "data-carousel-interval",
  );
  expect(contract.conventions.tabs.attribute).toBe("data-tabs");
  expect(contract.conventions.tabs.usage).toContain("data-tab-panel");
  expect(kitCss).toContain(".sb-carousel");
  expect(kitCss).toContain("[data-tab-panel][hidden]");
  // The attributes the enhancers key on must survive sanitization.
  const markup = sanitizeHtml(
    '<div data-carousel data-carousel-interval="5000"></div>' +
      '<div data-tabs><button data-tab="a">A</button><div data-tab-panel="a">A</div></div>',
  );
  expect(markup.querySelector("[data-carousel-interval]")).not.toBeNull();
  expect(markup.querySelector("[data-tabs] [data-tab]")).not.toBeNull();
  expect(markup.querySelector("[data-tabs] [data-tab-panel]")).not.toBeNull();
});

test("context-menu convention matches the parser's contract", () => {
  const menu = contract.conventions.contextMenu;
  expect(menu.attribute).toBe(CONTEXT_ITEMS_ATTR);
  // Every limit an agent must respect is stated with its real number.
  for (const limit of [
    CONTEXT_MENU_MAX_ITEMS,
    CONTEXT_MENU_MAX_LABEL_CHARS,
    CONTEXT_MENU_MAX_ACTION_CHARS,
    CONTEXT_MENU_MAX_VALUE_CHARS,
    CONTEXT_MENU_MAX_SPEC_CHARS,
  ]) {
    expect(menu.usage, `limit ${String(limit)} undocumented`).toContain(
      String(limit),
    );
  }
  // The full item vocabulary, so a fresh agent needs no other source.
  for (const field of [
    "label",
    "action",
    "value",
    "icon",
    "danger",
    "disabled",
    "checked",
    "separator",
    "items",
  ]) {
    expect(menu.usage, `field ${field} undocumented`).toContain(field);
  }
  expect(menu.usage).toContain("ONE submenu level");
  // The documented example must survive sanitization AND parse for real.
  const markup = sanitizeHtml(menu.example);
  const spec = markup
    .querySelector(`[${CONTEXT_ITEMS_ATTR}]`)
    ?.getAttribute(CONTEXT_ITEMS_ATTR);
  expect(spec).toBeTruthy();
  const items = parseContextItems(spec ?? "", "demo", "tile");
  expect(items?.length).toBe(4);
  expect(items?.some((item) => "items" in item)).toBe(true);
});

test("tooltip convention matches the tooltip layer", () => {
  const tooltip = contract.conventions.tooltip;
  expect(tooltip.attribute).toBe("title");
  expect(tooltip.usage).toContain(String(TOOLTIP_DELAY_MS));
  expect(tooltip.usage).toContain("aria-label");
  // The enhancer moves `title` onto this attribute, so it must be themeable.
  expect(overlayCss).toContain("--sb-tooltip-bg");
  expect(TOOLTIP_ATTR).toBe("data-sb-tooltip");
  // A plugin's title survives sanitization to reach the enhancer at all.
  expect(sanitizeHtml(tooltip.example).querySelector("[title]")).not.toBeNull();
});

test("contract documents popup stacking, sticky ttl, and tile chrome", () => {
  const popup = contract.renderTargets.popup;
  expect(popup).toContain("ttlMs");
  expect(popup).toContain("STICKY");
  expect(popup).toContain(
    `${String(POPUP_TTL_MIN_MS)}–${String(POPUP_TTL_MAX_MS)}`,
  );
  expect(popup).toContain(`up to ${String(POPUP_VISIBLE_MAX)} visible`);
  expect(popup).toContain("45 queued");
  expect(popup).toContain("keyboard focus");
  expect(popup).toContain("popups.position");
  expect(contract.renderTargets.hover).toContain("hoverPeek");
  expect(contract.renderTargets.hover).toContain("disabled");
  expect(contract.tileChrome.globalConfig).toContain("appearance.tileChrome");
  expect(contract.tileChrome.manifestOverride).toContain("wins");
});

test("contract carries compact desktop design rules and the media contract", () => {
  expect(contract.designRules.length).toBeGreaterThan(0);
  expect(contract.designRules.join(" ")).toContain("desktop utility panels");
  expect(contract.designRules.join(" ")).toContain("340px");
  expect(contract.media.links).toContain("system browser");
  expect(contract.media.videos).toContain("muted");
});

test("nothing hijacks the attribute smabar's own tooltip layer uses", () => {
  // The harvest renames data-f48-* to data-sb-*, and format48's CSS-only
  // tooltip uses data-f48-tooltip — the very attribute enhanceTooltips
  // writes. Harvesting it rendered a SECOND tooltip as a ::after pseudo
  // element: clipped by every overflow container and in the wrong colours.
  expect(harvestedCss).not.toContain(TOOLTIP_ATTR);
});

test("a long tooltip wraps instead of being cut off", () => {
  const tooltip = /\.overlay-tooltip \{([^}]*)\}/.exec(overlayCss)?.[1] ?? "";
  expect(tooltip).not.toBe("");
  // Tile descriptions are sentences; nowrap + ellipsis truncated them.
  expect(tooltip).not.toContain("text-overflow");
  expect(tooltip).not.toContain("white-space: nowrap");
  expect(tooltip).toContain("max-width");
});

test("the tooltip follows the opaque menu surface and current text", () => {
  expect(defaultTheme["--sb-tooltip-bg"]).toBe(defaultTheme["--sb-menu-bg"]);
  expect(defaultTheme["--sb-tooltip-text"]).toBe("var(--sb-text)");
});

test("the contract tells authors where removed markup is reported", () => {
  // A silent removal is the worst failure mode in the kit: the element is
  // simply absent. The contract must point at the log entry that explains it.
  expect(contract.sanitizer.reporting).toContain("plugin_logs");
  expect(contract.sanitizer.reporting).toContain("shell");
  expect(contract.sanitizer.reporting).toContain("once");
});

test("element base rules never outweigh single-class kit rules", () => {
  // A bare `.sb-root button` reset is (0,1,1) and silently beats .sb-btn
  // (0,1,0) — element base rules must use `:where(.sb-root) <element>`
  // (0,0,1) instead. `.sb-root *` (universal) stays allowed.
  const withoutComments = kitCss.replace(/\/\*[\s\S]*?\*\//g, "");
  expect(withoutComments).not.toMatch(/\.sb-root [a-z]/);
});

test("every themeable var(--sb-*) in the kit is a bundled theme token", () => {
  const tokens = new Set(Object.keys(defaultTheme));
  // A var() WITH a fallback is a documented opt-in knob, and a component may
  // set its own variable on a parent and read it on a child — neither has to
  // be a theme token. Only a bare var() must resolve.
  const declared = new Set(
    [...allKitCss.matchAll(/^\s*(--sb-[a-z0-9-]+)\s*:/gm)].map(
      (match) => match[1] ?? "",
    ),
  );
  const used = [...allKitCss.matchAll(/var\(\s*(--sb-[a-z0-9-]+)\s*\)/g)]
    .map((match) => match[1] ?? "")
    .filter((token) => !RUNTIME_VARS.has(token) && !declared.has(token));
  expect(used.length).toBeGreaterThan(0);
  for (const token of used) {
    expect(tokens.has(token), `${token} missing`).toBe(true);
  }
});

/** Generated internal/runtime tokens are deliberately absent from themes. */
const RUNTIME_VARS = new Set(Object.keys(componentTokens.internalTokens));
const THEMEABLE_VARS = new Set([
  ...Object.keys(defaultTheme),
  ...Object.entries(componentTokens.publicComponentTokens)
    .filter(([, definition]) => definition.themeable)
    .map(([name]) => name),
]);
const DOCUMENTED_VARS = new Set([...THEMEABLE_VARS, ...RUNTIME_VARS]);

test("every var(--sb-*) in overlay.css is documented", () => {
  const used = [...overlayCss.matchAll(/var\(\s*(--sb-[a-z0-9-]+)/g)].map(
    (match) => match[1] ?? "",
  );
  expect(used.length).toBeGreaterThan(0);
  for (const token of used) {
    expect(
      DOCUMENTED_VARS.has(token),
      `${token} missing in token contract`,
    ).toBe(true);
  }
});

test("every themeable var(--sb-*) in bar.css is documented", () => {
  const used = [...barCss.matchAll(/var\(\s*(--sb-[a-z0-9-]+)/g)]
    .map((match) => match[1] ?? "")
    .filter((token) => !RUNTIME_VARS.has(token));
  expect(used.length).toBeGreaterThan(0);
  for (const token of used) {
    expect(
      THEMEABLE_VARS.has(token),
      `${token} missing in token contract`,
    ).toBe(true);
  }
});

function tagSequence(root: ParentNode): string[] {
  return [...root.querySelectorAll("*")].map((el) => el.tagName.toLowerCase());
}

test("every contract snippet survives sanitizeHtml structurally", () => {
  for (const [name, snippet] of Object.entries(contract.snippets)) {
    const parsed = document.createElement("template");
    parsed.innerHTML = snippet;
    const before = tagSequence(parsed.content);
    expect(before.length, `snippet ${name} parses to elements`).toBeGreaterThan(
      0,
    );

    const container = document.createElement("div");
    container.appendChild(sanitizeHtml(snippet));
    expect(tagSequence(container), `snippet ${name} lost elements`).toEqual(
      before,
    );
    // The interaction wiring must survive too.
    const actionsBefore = parsed.content.querySelectorAll("[data-action]");
    const actionsAfter = container.querySelectorAll("[data-action]");
    expect(actionsAfter.length, `snippet ${name} lost data-action`).toBe(
      actionsBefore.length,
    );
  }
});

test("the native-interaction snippets keep the wiring that makes them work", () => {
  // These three are the only way a plugin gets an accordion, a modal or a
  // menu — it ships no JavaScript. The tag surviving is not enough: the
  // attribute that drives the behaviour has to survive with it.
  const accordion = sanitizeHtml(contract.snippets.accordion);
  expect(accordion.querySelector("details > summary")).not.toBeNull();
  expect(accordion.querySelector("details[open]")).not.toBeNull();

  const modal = sanitizeHtml(contract.snippets.modal);
  const opener = modal.querySelector("button[command='show-modal']");
  expect(opener?.getAttribute("commandfor")).toBe("confirm-clear");
  expect(modal.querySelector("dialog")?.id).toBe("confirm-clear");
  expect(modal.querySelectorAll("button[command='close']").length).toBe(2);

  const menu = sanitizeHtml(contract.snippets.popoverMenu);
  const trigger = menu.querySelector("button[popovertarget]");
  expect(trigger?.getAttribute("popovertarget")).toBe("feed-menu");
  const card = menu.querySelector("[popover]");
  expect(card?.id).toBe("feed-menu");
  expect(menu.querySelectorAll("[popovertargetaction='hide']").length).toBe(2);

  // Every command value the snippets use has to be one the shell honours;
  // a custom command fires an event nothing listens for.
  for (const element of [...modal.querySelectorAll("[command]")]) {
    expect(COMMAND_VALUES.has(element.getAttribute("command") ?? "")).toBe(
      true,
    );
  }
});

test("the data table snippet keeps its table structure", () => {
  const table = sanitizeHtml(contract.snippets.dataTable);
  expect(table.querySelector(".sb-table-wrap > table.sb-table")).not.toBeNull();
  expect(table.querySelectorAll("thead th").length).toBe(3);
  expect(table.querySelectorAll("tbody tr").length).toBe(2);
  expect(table.querySelector("th[scope='col']")).not.toBeNull();
});
