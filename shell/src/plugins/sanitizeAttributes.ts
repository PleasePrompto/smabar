import { ASSET_SCHEME } from "./assets";
import type { DropReporter } from "./sanitizeEmbed";
import {
  ALLOWED_ATTRS,
  AUDIO_ATTRS,
  AUTOFOCUS_TAGS,
  BUTTON_TYPES,
  COMMAND_VALUES,
  CONTROL_ATTRS,
  CONTROL_TAGS,
  DATA_ATTR_PATTERN,
  FORM_BLOCKED_ATTRS,
  INPUT_TYPES,
  NATIVE_INTERACTION_ATTRS,
  SIZED_TAGS,
  SOURCE_ATTRS,
  TRACK_ATTRS,
  TRACK_KINDS,
  VIDEO_ATTRS,
} from "./sanitizeRules";
import { hasJavascriptUrl, isHttpUrl, isSafeMediaSrc } from "./sanitizeUrls";

/** Turns an `sb-asset:` URL into a loadable one, or null when unsafe. */
export type AssetResolver = (value: string) => string | null;

export function copyAllowedAttribute(
  tag: string,
  attr: Attr,
  /** The ORIGINAL element — its ancestors decide whether autofocus is kept. */
  source: Element,
  target: Element,
  resolveSrc: AssetResolver,
  onDrop: DropReporter,
): void {
  const name = attr.name.toLowerCase();
  const drop = (reason: string): void => {
    onDrop({ what: `${tag}[${name}]`, reason });
  };
  // javascript: URLs are refused in EVERY attribute value.
  if (hasJavascriptUrl(attr.value)) {
    drop(
      "a javascript: URL is never copied. Use data-action and handle the click with @app.on_action.",
    );
    return;
  }
  if (
    name === "src" ||
    (tag === "video" && name === "poster" && VIDEO_ATTRS.has(name))
  ) {
    const imageSource = tag === "img" || name === "poster";
    // `sb-asset:` addresses a file the plugin wrote into its own data
    // directory; the resolver validates the path and returns null when it
    // could escape, so an unusable one simply drops the attribute.
    if (attr.value.trim().toLowerCase().startsWith(ASSET_SCHEME)) {
      if (tag === "track") {
        drop(
          `${ASSET_SCHEME} cannot serve WebVTT with the required text/vtt type. Read the local file in the plugin and render it as a data:text/vtt URI.`,
        );
        return;
      }
      const resolved = resolveSrc(attr.value.trim());
      if (resolved === null) {
        drop(
          `${ASSET_SCHEME} paths are relative to your own data directory: no "..", no absolute path, no query string.`,
        );
        return;
      }
      target.setAttribute(name, resolved);
      return;
    }
    const sourceParent = source.parentElement?.tagName.toLowerCase();
    const audioSource =
      tag === "audio" || (tag === "source" && sourceParent === "audio");
    const trackSource = tag === "track";
    const safe = imageSource
      ? isSafeMediaSrc(attr.value, "image/")
      : audioSource
        ? isSafeMediaSrc(attr.value, "audio/")
        : trackSource
          ? isSafeMediaSrc(attr.value, "text/vtt")
          : (tag === "video" || tag === "source") &&
            isSafeMediaSrc(attr.value, "video/");
    if (safe) {
      target.setAttribute(name, attr.value);
      return;
    }
    const dataType = imageSource
      ? "image/"
      : audioSource
        ? "audio/"
        : trackSource
          ? "text/vtt"
          : "video/";
    drop(
      `${name} must start with https://, http:// or data:${dataType}${trackSource ? "" : ` or ${ASSET_SCHEME}`}`,
    );
    return;
  }
  if (name === "href") {
    // Inside a lightbox gallery the href is not a destination but the full
    // image the viewer opens — the click never navigates, the behaviour
    // layer takes it. It goes through the same resolver as any other asset,
    // so a path that could escape the plugin's own folder still drops.
    if (tag === "a" && source.closest("[data-sb-lightbox]") !== null) {
      const value = attr.value.trim();
      if (value.toLowerCase().startsWith(ASSET_SCHEME)) {
        const resolved = resolveSrc(value);
        if (resolved !== null) target.setAttribute("href", resolved);
        else
          drop(
            `${ASSET_SCHEME} path must stay inside the plugin's own data directory.`,
          );
        return;
      }
      if (isHttpUrl(value) && isSafeMediaSrc(value, "image/")) {
        target.setAttribute("href", value);
        return;
      }
      drop(
        `a lightbox image must use http://, https:// or a safe ${ASSET_SCHEME} path.`,
      );
      return;
    }
    // http(s) only; the shell opens links in the system browser.
    if (tag === "a" && isHttpUrl(attr.value)) {
      target.setAttribute("href", attr.value);
      return;
    }
    drop(
      "only <a href> with http:// or https:// is kept; the shell opens it in the system browser.",
    );
    return;
  }
  if ((tag === "audio" || tag === "video") && name === "crossorigin") {
    const value = attr.value.toLowerCase();
    if (value === "" || value === "anonymous") {
      target.setAttribute("crossorigin", "anonymous");
      return;
    }
    drop(
      'crossorigin supports only "anonymous"; credentialed media requests are not allowed.',
    );
    return;
  }
  if (tag === "video" && VIDEO_ATTRS.has(name)) {
    target.setAttribute(name, attr.value);
    return;
  }
  if (tag === "audio" && AUDIO_ATTRS.has(name)) {
    target.setAttribute(name, attr.value);
    return;
  }
  if (tag === "source" && SOURCE_ATTRS.has(name)) {
    target.setAttribute(name, attr.value);
    return;
  }
  if (tag === "track" && TRACK_ATTRS.has(name)) {
    if (name !== "kind" || TRACK_KINDS.has(attr.value.toLowerCase())) {
      target.setAttribute(
        name,
        name === "kind" ? attr.value.toLowerCase() : attr.value,
      );
      return;
    }
    drop(
      `track kind "${attr.value}" is unsupported. Use one of: ${[...TRACK_KINDS].join(", ")}.`,
    );
    return;
  }
  if ((name === "width" || name === "height") && SIZED_TAGS.has(tag)) {
    // Digits only: a percentage or "auto" belongs in style, and anything
    // else here would be a way to smuggle CSS past the style handling.
    if (/^\d+$/.test(attr.value)) {
      target.setAttribute(name, attr.value);
      return;
    }
    drop(
      `${name}="${attr.value}" must be a plain pixel count; use style for anything else.`,
    );
    return;
  }
  if (name === "autofocus") {
    // Only inside a dialog: elsewhere it would pull focus on every render,
    // and a plugin re-renders every second.
    if (AUTOFOCUS_TAGS.has(tag) && source.closest("dialog") !== null) {
      target.setAttribute("autofocus", "");
      return;
    }
    drop(
      "autofocus is only kept on a control inside a <dialog>; elsewhere it would steal focus on every re-render.",
    );
    return;
  }
  if (tag === "button" && name === "type") {
    if (BUTTON_TYPES.has(attr.value.toLowerCase())) {
      target.setAttribute("type", attr.value.toLowerCase());
      return;
    }
    drop(`button type "${attr.value}" is not a button type.`);
    return;
  }
  // <details name> groups panels into an EXCLUSIVE accordion — opening one
  // closes its siblings. Without it every panel is independent.
  if (tag === "details" && name === "name") {
    target.setAttribute("name", attr.value);
    return;
  }
  if (tag === "input" && name === "type") {
    const type = attr.value.toLowerCase();
    if (INPUT_TYPES.has(type)) {
      target.setAttribute("type", type);
      return;
    }
    drop(
      `input type "${type}" is not supported; it renders as text. Supported: ${[...INPUT_TYPES].join(", ")}.`,
    );
    return;
  }
  // A form may exist for grouping, labels and native validation, but must
  // never navigate — the shell routes its submit into data-action instead.
  if (tag === "form" && FORM_BLOCKED_ATTRS.has(name)) {
    drop(
      `<form ${name}> is removed; a tile never navigates. Submit is delivered to @app.on_action via the data-action on your submit button.`,
    );
    return;
  }
  if (CONTROL_TAGS.has(tag) && CONTROL_ATTRS.has(name)) {
    target.setAttribute(name, attr.value);
    return;
  }
  // Native interaction: popover and the dialog invoker commands. Only the
  // built-in command values do anything; a custom one (`--foo`) would fire an
  // event nothing listens for, which looks like it works and does not.
  if (NATIVE_INTERACTION_ATTRS.has(name)) {
    if (name === "command" && !COMMAND_VALUES.has(attr.value.toLowerCase())) {
      drop(
        `command "${attr.value}" does nothing here. Use one of: ${[...COMMAND_VALUES].join(", ")}.`,
      );
      return;
    }
    target.setAttribute(name, attr.value);
    return;
  }
  if (!ALLOWED_ATTRS.has(name) && !DATA_ATTR_PATTERN.test(name)) {
    drop(
      `attribute "${name}" is not allowed. Allowed: ${[...ALLOWED_ATTRS].join(", ")}, plus data-* and the control attributes on form elements.`,
    );
    return;
  }
  target.setAttribute(name, attr.value);
}
