import {
  EMBED_ALLOW,
  EMBED_REFERRER_POLICY,
  EMBED_SANDBOX,
} from "./sanitizeRules";

/** Rewrites a remote document into the core's isolated loopback frame. */
export type EmbedResolver = (value: string, title: string) => string | null;

/** One thing the sanitizer removed, and what the author should do instead. */
export interface SanitizerDrop {
  what: string;
  reason: string;
}

/** Called for everything the sanitizer removes. */
export type DropReporter = (drop: SanitizerDrop) => void;

/** Default reporter for callers that do not need removal diagnostics. */
export function noReport(): void {
  // Intentionally empty: reporting is optional at the sanitizer boundary.
}

type AttributeCopier = (attr: Attr, target: Element) => void;

const NORMALIZED_ATTRIBUTES = new Set([
  "allow",
  "allowfullscreen",
  "frameborder",
  "loading",
  "referrerpolicy",
  "sandbox",
  "src",
  "title",
]);

/**
 * Rebuilds a remote player as an empty, sandboxed frame whose URL points at
 * the core's unprivileged loopback wrapper. Provider markup may request broader
 * permissions, but the fixed values below always win.
 */
export function appendEmbed(
  source: Element,
  parent: Element | DocumentFragment,
  doc: Document,
  onDrop: DropReporter,
  resolveEmbed: EmbedResolver | null,
  copyAttribute: AttributeCopier,
): void {
  const clean = doc.createElement("iframe");
  for (const attr of Array.from(source.attributes)) {
    const name = attr.name.toLowerCase();
    if (name === "srcdoc") {
      onDrop({
        what: "iframe[srcdoc]",
        reason:
          "inline frame documents never run. Use a credential-free HTTPS provider embed URL in iframe[src].",
      });
    } else if (!NORMALIZED_ATTRIBUTES.has(name)) {
      copyAttribute(attr, clean);
    } else if (
      (name === "allow" && attr.value !== EMBED_ALLOW) ||
      (name === "sandbox" && attr.value !== EMBED_SANDBOX) ||
      (name === "referrerpolicy" && attr.value !== EMBED_REFERRER_POLICY) ||
      (name === "loading" && attr.value !== "lazy") ||
      name === "frameborder"
    ) {
      onDrop({
        what: `iframe[${name}]`,
        reason: `${name} is fixed by the shell for isolated media playback; remove this attribute from plugin markup.`,
      });
    }
  }

  const title = source.getAttribute("title")?.trim() ?? "";
  if (title === "") {
    onDrop({
      what: "iframe[title]",
      reason:
        "an embedded player needs a non-empty title so keyboard and screen-reader users know what it contains.",
    });
    return;
  }
  const sourceUrl = source.getAttribute("src")?.trim() ?? "";
  if (!sourceUrl.toLowerCase().startsWith("https://")) {
    onDrop({
      what: "iframe[src]",
      reason:
        "an embedded player src must be a credential-free https:// URL. Local files belong in img, audio or video via sb-asset:.",
    });
    return;
  }
  if (resolveEmbed === null) {
    onDrop({
      what: "iframe[src]",
      reason:
        "embedded players are interactive and are kept only in a pinned flyout. Render a thumbnail in the tile, hover preview or popup.",
    });
    return;
  }
  const wrapped = resolveEmbed(sourceUrl, title);
  if (wrapped === null) {
    onDrop({
      what: "iframe[src]",
      reason:
        "the HTTPS player URL could not be isolated. Remove URL credentials, keep src at 8192 characters or fewer and title at 512 or fewer, then use plugin_logs if the embed service is unavailable.",
    });
    return;
  }

  clean.setAttribute("src", wrapped);
  clean.setAttribute("title", title);
  clean.setAttribute("loading", "lazy");
  clean.setAttribute("allow", EMBED_ALLOW);
  clean.setAttribute("allowfullscreen", "");
  clean.setAttribute("referrerpolicy", EMBED_REFERRER_POLICY);
  clean.setAttribute("sandbox", EMBED_SANDBOX);
  parent.appendChild(clean);
}
