/**
 * What plugin markup may contain.
 *
 * Split from `sanitize.ts` so the rules read as one table and the logic stays
 * short. Both are mirrored in `ui-kit/contract.json` (served to agents via the
 * MCP `ui_kit` tool); an anti-drift test keeps them equal in both directions.
 *
 * The line between allowed and forbidden is NOT "could a plugin do damage" —
 * a plugin is a full user process and could already do anything. It is "could
 * this run code in the SHELL's document", because that document holds
 * `window.__TAURI_INTERNALS__` and therefore every command, regardless of the
 * plugin protocol. So script, event handlers and stylesheets stay out;
 * foreign documents only survive through the shell's isolated HTTP wrapper.
 */

/**
 * Tags that survive sanitization.
 *
 * Deliberately generous: `<details>`, `<dialog>`, `<table>`, `<select>` and
 * friends are inert, and they are what lets a plugin be genuinely interactive
 * WITHOUT shipping a line of JavaScript — see NATIVE_INTERACTION_ATTRS.
 */
export const ALLOWED_TAGS = new Set([
  // text
  "a",
  "abbr",
  "b",
  "blockquote",
  "br",
  "code",
  "em",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "hr",
  "i",
  "kbd",
  "mark",
  "p",
  "pre",
  "s",
  "samp",
  "small",
  "span",
  "strong",
  "sub",
  "sup",
  "time",
  "u",
  "var",
  "wbr",
  // grouping
  "article",
  "aside",
  "div",
  "figcaption",
  "figure",
  "footer",
  "header",
  "main",
  "nav",
  "section",
  // lists
  "dd",
  "dl",
  "dt",
  "li",
  "ol",
  "ul",
  // tables
  "caption",
  "col",
  "colgroup",
  "table",
  "tbody",
  "td",
  "tfoot",
  "th",
  "thead",
  "tr",
  // forms and controls
  "button",
  "datalist",
  "fieldset",
  "form",
  "input",
  "label",
  "legend",
  "meter",
  "optgroup",
  "option",
  "output",
  "progress",
  "select",
  "textarea",
  // interactive containers (native behaviour, no JS)
  "details",
  "dialog",
  "summary",
  // media
  "audio",
  "iframe",
  "img",
  "picture",
  "source",
  "track",
  "video",
]);

/** Tags removed together with their entire content. */
export const DROPPED_TAGS = new Set([
  "embed",
  "link",
  "meta",
  "object",
  "script",
  "style",
  "template",
]);

/**
 * Attributes allowed on any allowed tag.
 *
 * `id` is in here on purpose: a shadow root scopes ids to its own tree, so a
 * plugin cannot reach anything outside itself with one — while `id`/`for`
 * and the aria relations are what make a form or a tab list accessible at
 * all. `src`/`href` are handled separately.
 */
export const ALLOWED_ATTRS = new Set([
  "alt",
  "aria-busy",
  "aria-controls",
  "aria-current",
  "aria-describedby",
  "aria-disabled",
  "aria-expanded",
  "aria-haspopup",
  "aria-hidden",
  "aria-label",
  "aria-labelledby",
  "aria-live",
  "aria-modal",
  "aria-pressed",
  "aria-roledescription",
  "aria-selected",
  "datetime",
  "class",
  "colspan",
  "dir",
  "for",
  "headers",
  "hidden",
  "id",
  "lang",
  "placeholder",
  "role",
  "rowspan",
  "scope",
  "span",
  // Deliberate: plugins style their markup (data-marquee widths, inline
  // progress bars). The threat model is integrity, not visual containment: a
  // plugin is already a full user process, and Shadow DOM does not constrain
  // fixed-position descendants. Do NOT remove without breaking every bundled
  // plugin's markup.
  "style",
  "tabindex",
  "title",
]);

/**
 * Native interaction without JavaScript.
 *
 * `<button popovertarget>` opens a `[popover]`, and `<button commandfor>`
 * with a built-in `command` opens or closes a `<dialog>`. Both are pure
 * HTML — which is the whole reason a plugin that can only emit strings can
 * still have menus and modals.
 */
export const NATIVE_INTERACTION_ATTRS = new Set([
  "command",
  "commandfor",
  "open",
  "popover",
  "popovertarget",
  "popovertargetaction",
]);

/**
 * The `command` values that are honoured.
 *
 * Only built-ins. A custom command (anything starting with `--`) fires a
 * `CommandEvent` that nothing in the shell listens for, so it would be a
 * silent no-op that looks like it should work.
 */
export const COMMAND_VALUES = new Set([
  "close",
  "hide-popover",
  "request-close",
  "show-modal",
  "show-popover",
  "toggle-popover",
]);

/** Generic data-attribute rule: covers the plugin protocol (data-action,
 *  data-field, data-value) and the kit conventions (data-lucide,
 *  data-chart, data-points, …). */
export const DATA_ATTR_PATTERN = /^data-[a-z0-9-]+$/;

/**
 * `type` values a button may carry.
 *
 * Agents write `<button type="button">` by reflex. Stripping it was harmless
 * behaviourally — a bare button already defaults to `submit` only inside a
 * form, which the shell intercepts — but it made every such plugin report a
 * removal on its first render, which is noise in the one channel that must
 * stay trustworthy.
 */
export const BUTTON_TYPES = new Set(["button", "reset", "submit"]);

/** `type` values an input may carry; anything else renders as text. */
export const INPUT_TYPES = new Set([
  "checkbox",
  "color",
  "date",
  "datetime-local",
  "email",
  "month",
  "number",
  "password",
  "radio",
  "range",
  "search",
  "tel",
  "text",
  "time",
  "url",
  "week",
]);

/**
 * Tags that may carry intrinsic `width`/`height`.
 *
 * On an image they reserve the right box before the file loads, which is the
 * difference between a flyout that settles and one that jumps. On a table
 * cell or a div the same attributes are presentational legacy, so they stay
 * refused there.
 */
export const SIZED_TAGS = new Set(["iframe", "img", "video", "canvas"]);

/**
 * Tags where `autofocus` is honoured.
 *
 * Inside a `<dialog>` it is the only way to put the caret in the first field
 * when it opens — a command palette without it opens with focus nowhere.
 * Anywhere else it would steal focus on every re-render, so it is refused.
 */
export const AUTOFOCUS_TAGS = new Set([
  "button",
  "input",
  "select",
  "textarea",
]);

/** Attributes allowed on form controls only (input, select, textarea, …). */
export const CONTROL_ATTRS = new Set([
  "checked",
  // Picks the on-screen keyboard on a touch device; inert on the desktop but
  // correct on a numeric code field, and a plugin may run on a tablet.
  "inputmode",
  "cols",
  "disabled",
  "list",
  "max",
  "maxlength",
  "min",
  "minlength",
  "multiple",
  "name",
  "readonly",
  "required",
  "rows",
  "selected",
  "size",
  "step",
  "value",
]);

/** Tags that accept {@link CONTROL_ATTRS}. */
export const CONTROL_TAGS = new Set([
  "button",
  "fieldset",
  "input",
  "meter",
  "optgroup",
  "option",
  "output",
  "progress",
  "select",
  "textarea",
]);

/**
 * Attributes stripped from `<form>`.
 *
 * A form may exist (for grouping, labels and native validation) but must
 * never navigate: the shell intercepts `submit` and routes it into the
 * existing data-action pipeline instead.
 */
export const FORM_BLOCKED_ATTRS = new Set([
  "action",
  "enctype",
  "formaction",
  "method",
  "novalidate",
  "target",
]);

/** Attributes allowed on video elements only ("src" is handled separately;
 *  "poster" follows the image URL rules). Mirrored as media.videoAttrs in
 *  ui-kit/contract.json. */
export const VIDEO_ATTRS = new Set([
  "autoplay",
  "controls",
  "crossorigin",
  "loop",
  "muted",
  "playsinline",
  "poster",
  "preload",
]);

/** Attributes allowed on native audio players. */
export const AUDIO_ATTRS = new Set([
  "autoplay",
  "controls",
  "crossorigin",
  "loop",
  "muted",
  "preload",
]);

/** Inert format hints on nested media sources. */
export const SOURCE_ATTRS = new Set(["media", "type"]);

/** Attributes on WebVTT subtitle/caption tracks. */
export const TRACK_ATTRS = new Set(["default", "kind", "label", "srclang"]);

export const TRACK_KINDS = new Set([
  "captions",
  "chapters",
  "descriptions",
  "metadata",
  "subtitles",
]);

/** Fixed on both the shell frame and unprivileged wrapper's provider frame. */
export const EMBED_ALLOW =
  "autoplay; encrypted-media; fullscreen; picture-in-picture";
export const EMBED_SANDBOX = "allow-same-origin allow-scripts";
export const EMBED_REFERRER_POLICY = "strict-origin-when-cross-origin";

/**
 * SVG elements a plugin may draw with.
 *
 * Shapes and text only. `use` (can reference another document), `script`,
 * `foreignObject` (re-enters HTML, bypassing every rule above), `animate` and
 * `set` are deliberately absent — those are the ways out of an SVG subtree.
 */
export const SVG_TAGS = new Set([
  "circle",
  "defs",
  "ellipse",
  "g",
  "line",
  "linearGradient",
  "path",
  "polygon",
  "polyline",
  "radialGradient",
  "rect",
  "stop",
  "svg",
  "text",
  "tspan",
]);

/**
 * Attributes allowed inside an SVG subtree. Case matters here (`viewBox`),
 * unlike in HTML — the sanitizer therefore matches these verbatim.
 */
export const SVG_ATTRS = new Set([
  "aria-hidden",
  "aria-label",
  "aria-labelledby",
  "class",
  "cx",
  "cy",
  "d",
  "dx",
  "dy",
  "fill",
  "fill-opacity",
  "fill-rule",
  "gradientUnits",
  "height",
  "id",
  "offset",
  "opacity",
  "points",
  "r",
  "role",
  "rx",
  "ry",
  "stop-color",
  "stop-opacity",
  "stroke",
  "stroke-dasharray",
  "stroke-dashoffset",
  "stroke-linecap",
  "stroke-linejoin",
  "stroke-opacity",
  "stroke-width",
  "style",
  "text-anchor",
  "transform",
  "viewBox",
  "width",
  "x",
  "x1",
  "x2",
  "y",
  "y1",
  "y2",
]);

export const SVG_NAMESPACE = "http://www.w3.org/2000/svg";
