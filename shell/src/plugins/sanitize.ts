/**
 * Allowlist sanitizer for plugin-rendered HTML.
 *
 * This is NOT a security boundary — plugins already run with full system
 * rights out of process. It protects the shell's integrity: plugin markup
 * must never inject scripts, navigable forms or stylesheets into the bar.
 * Remote frames are rewritten to an isolated loopback origin. Kept
 * deliberately small and strict; anything not allowlisted is dropped.
 *
 * The allowlists below are mirrored in ui-kit/contract.json (served to
 * agents via the MCP `ui_kit` tool); an anti-drift test keeps them equal.
 */

import {
  appendEmbed,
  type DropReporter,
  type EmbedResolver,
  noReport,
} from "./sanitizeEmbed";
import { copyAllowedAttribute, type AssetResolver } from "./sanitizeAttributes";
import {
  ALLOWED_ATTRS,
  ALLOWED_TAGS,
  AUDIO_ATTRS,
  BUTTON_TYPES,
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
  SVG_ATTRS,
  SVG_NAMESPACE,
  SVG_TAGS,
  TRACK_ATTRS,
  TRACK_KINDS,
  VIDEO_ATTRS,
} from "./sanitizeRules";
import { hasJavascriptUrl } from "./sanitizeUrls";

export type { AssetResolver } from "./sanitizeAttributes";
export type { EmbedResolver, SanitizerDrop } from "./sanitizeEmbed";

// Re-exported so callers and the anti-drift test keep one import site.
export {
  ALLOWED_ATTRS,
  ALLOWED_TAGS,
  AUDIO_ATTRS,
  BUTTON_TYPES,
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
};

/** Why a dropped tag is dropped, in terms of the supported alternative. */
function dropReason(tag: string): string {
  switch (tag) {
    case "script":
      return "<script> is dropped and never runs. Compute in the plugin process and render the result.";
    case "style":
      return "<style> is dropped. Use the sb-* kit classes and --sb-* tokens instead of your own CSS.";
    case "template":
      return "<template> is dropped; its content would never render. Emit the markup you want directly.";
    case "embed":
    case "object":
      return `<${tag}> is dropped. Use a titled iframe with the provider's HTTPS embed URL in a pinned flyout.`;
    default:
      return `<${tag}> is dropped by the sanitizer.`;
  }
}

/**
 * True when the HTML parser put this element in the SVG namespace.
 *
 * DOMParser does that for `<svg>` and everything inside it, so namespace —
 * not the tag name — is what tells the two trees apart.
 */
function isSvg(node: Element): boolean {
  return node.namespaceURI === SVG_NAMESPACE;
}

/**
 * Rebuilds one SVG element in the SVG namespace.
 *
 * Separate from the HTML path for three reasons: elements need
 * `createElementNS` or they render as unknown HTML, attribute names are
 * case-sensitive, and the tag list is much narrower — `use`, `script` and
 * `foreignObject` are the ways out of an SVG subtree and never appear in
 * SVG_TAGS.
 */
function appendSvg(
  source: Element,
  parent: Element | DocumentFragment,
  doc: Document,
  onDrop: DropReporter,
): void {
  const tag = source.localName;
  if (!SVG_TAGS.has(tag)) {
    onDrop({
      what: tag,
      reason: `<${tag}> is not allowed inside <svg>. Shapes, text and gradients are; use, script and foreignObject are not.`,
    });
    return;
  }
  const clean = doc.createElementNS(SVG_NAMESPACE, tag);
  for (const attr of Array.from(source.attributes)) {
    if (hasJavascriptUrl(attr.value)) {
      onDrop({
        what: `${tag}[${attr.name}]`,
        reason:
          "a javascript: URL is never copied into SVG. Use a literal presentation value or a local url(#gradient).",
      });
      continue;
    }
    if (SVG_ATTRS.has(attr.name) || DATA_ATTR_PATTERN.test(attr.name)) {
      clean.setAttribute(attr.name, attr.value);
      continue;
    }
    onDrop({
      what: `${tag}[${attr.name}]`,
      reason: `attribute "${attr.name}" is not allowed inside <svg>.`,
    });
  }
  for (const child of [...source.childNodes]) {
    if (child.nodeType === Node.TEXT_NODE) {
      clean.appendChild(doc.createTextNode(child.textContent ?? ""));
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue;
    appendSvg(child as Element, clean, doc, onDrop);
  }
  parent.appendChild(clean);
}

/**
 * Parses untrusted plugin HTML in a detached template and rebuilds it as a
 * fresh, allowlisted DocumentFragment. Unlike DOMParser, template contents
 * have no browsing context, so refused frames cannot fetch before inspection.
 */
export function sanitizeHtml(
  html: string,
  resolveSrc: AssetResolver = () => null,
  onDrop: DropReporter = noReport,
  resolveEmbed: EmbedResolver | null = null,
): DocumentFragment {
  const template = document.createElement("template");
  template.innerHTML = html;
  const fragment = document.createDocumentFragment();
  for (const child of [...template.content.childNodes]) {
    appendSanitized(
      child,
      fragment,
      document,
      resolveSrc,
      onDrop,
      resolveEmbed,
      false,
    );
  }
  return fragment;
}

function appendSanitized(
  node: Node,
  parent: Element | DocumentFragment,
  doc: Document,
  resolveSrc: AssetResolver,
  onDrop: DropReporter,
  resolveEmbed: EmbedResolver | null,
  inSvg: boolean,
): void {
  if (node.nodeType === Node.TEXT_NODE) {
    parent.appendChild(doc.createTextNode(node.textContent ?? ""));
    return;
  }
  // Comments, CDATA, processing instructions, … carry no plugin UI.
  if (node.nodeType !== Node.ELEMENT_NODE) return;

  const source = node as Element;
  const tag = source.tagName.toLowerCase();
  if (DROPPED_TAGS.has(tag)) {
    onDrop({ what: tag, reason: dropReason(tag) });
    return;
  }
  // An <svg> opens a subtree with its own namespace, its own tag list and
  // case-sensitive attributes; everything below it takes the SVG branch.
  if (inSvg || isSvg(source)) {
    appendSvg(source, parent, doc, onDrop);
    return;
  }
  if (!ALLOWED_TAGS.has(tag)) {
    // Unknown-but-harmless tags (table, section, h5, …) are unwrapped: the
    // tag goes, its sanitized children stay.
    onDrop({
      what: tag,
      reason: `<${tag}> is not part of the kit; it was unwrapped and only its children remain. Use a div with an sb-* class.`,
    });
    for (const child of [...source.childNodes]) {
      appendSanitized(
        child,
        parent,
        doc,
        resolveSrc,
        onDrop,
        resolveEmbed,
        false,
      );
    }
    return;
  }

  if (tag === "iframe") {
    appendEmbed(source, parent, doc, onDrop, resolveEmbed, (attr, target) => {
      copyAllowedAttribute("iframe", attr, source, target, resolveSrc, onDrop);
    });
    return;
  }

  // Rebuild instead of mutating: leftover namespaces, properties or
  // unforeseen attribute quirks of the parsed node can never leak through.
  const clean = doc.createElement(tag);
  for (const attr of Array.from(source.attributes)) {
    copyAllowedAttribute(tag, attr, source, clean, resolveSrc, onDrop);
  }
  // Autoplay is only honored muted (webview policy) — and a toast must
  // never blast audio. Moving video also remains pausable, and starts still
  // when the user has asked the OS to reduce motion.
  if ((tag === "audio" || tag === "video") && clean.hasAttribute("autoplay")) {
    clean.setAttribute("muted", "");
    if (tag === "video") {
      clean.setAttribute("controls", "");
      if (
        typeof window.matchMedia === "function" &&
        window.matchMedia("(prefers-reduced-motion: reduce)").matches
      ) {
        clean.removeAttribute("autoplay");
      }
    }
  }
  for (const child of [...source.childNodes]) {
    appendSanitized(child, clean, doc, resolveSrc, onDrop, resolveEmbed, false);
  }
  parent.appendChild(clean);
}
