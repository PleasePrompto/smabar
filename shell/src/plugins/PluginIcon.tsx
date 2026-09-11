import { useEffect, useRef, type CSSProperties, type ReactNode } from "react";

import { reportMarkupDrops } from "./markupReport";
import { sanitizeHtml, type SanitizerDrop } from "./sanitize";
import { SVG_NAMESPACE } from "./sanitizeRules";

const MAX_ICON_BYTES = 8 * 1024;
const LOCAL_PAINT_URL = /^url\(\s*#[A-Za-z_][\w.-]*\s*\)$/;

interface PluginIconProps {
  pluginId: string;
  tileId: string;
  /** Untrusted SVG source from a tile manifest. */
  svg?: string;
  /** Core-decoded 128px PNG. Never a filesystem path or remote URL. */
  dataUrl?: string | null;
  fallback?: ReactNode;
  /** Shell-owned layout classes; SVG-authored classes are stripped. */
  className?: string;
  style?: CSSProperties;
}

function sanitizedIcon(
  source: string,
  onDrop: (drop: SanitizerDrop) => void,
): Element | null {
  if (new TextEncoder().encode(source).byteLength > MAX_ICON_BYTES) {
    onDrop({
      what: "iconSvg",
      reason: `iconSvg exceeds ${String(MAX_ICON_BYTES)} UTF-8 bytes. Simplify the SVG paths or use a Lucide icon in the tile markup.`,
    });
    return null;
  }

  const fragment = sanitizeHtml(source, () => null, onDrop);
  const roots = [...fragment.childNodes].filter(
    (node) => node.nodeType !== Node.TEXT_NODE || node.textContent?.trim(),
  );
  if (roots.length !== 1) {
    onDrop({
      what: "iconSvg",
      reason:
        "iconSvg must contain exactly one <svg> root and no sibling content.",
    });
    return null;
  }
  const root = roots[0];
  if (
    !(root instanceof Element) ||
    root.namespaceURI !== SVG_NAMESPACE ||
    root.localName !== "svg"
  ) {
    onDrop({
      what: "iconSvg",
      reason:
        "iconSvg must contain exactly one <svg> root and no sibling content.",
    });
    return null;
  }

  for (const element of [root, ...root.querySelectorAll("*")]) {
    for (const name of ["class", "style"]) {
      if (!element.hasAttribute(name)) continue;
      element.removeAttribute(name);
      onDrop({
        what: `${element.localName}[${name}]`,
        reason: `SVG ${name} is removed from iconSvg. Use presentation attributes such as fill="currentColor" and stroke="currentColor".`,
      });
    }
    for (const attribute of [...element.attributes]) {
      const value = attribute.value.replace(/[\u0000-\u0020]/g, "");
      const lower = value.toLowerCase();
      if (attribute.name !== "fill" && attribute.name !== "stroke") continue;
      const referencesUrl =
        value.includes("\\") ||
        lower.includes("url(") ||
        lower.includes("://") ||
        lower.startsWith("//");
      if (!referencesUrl || LOCAL_PAINT_URL.test(value)) continue;
      element.removeAttribute(attribute.name);
      onDrop({
        what: `${element.localName}[${attribute.name}]`,
        reason:
          "external SVG paint URLs are removed from iconSvg. Use currentColor, a literal color, or a local url(#gradient).",
      });
    }
  }
  root.setAttribute("width", "1em");
  root.setAttribute("height", "1em");
  root.setAttribute("aria-hidden", "true");
  root.setAttribute("focusable", "false");
  root.removeAttribute("aria-label");
  root.removeAttribute("aria-labelledby");
  root.removeAttribute("role");
  return root;
}

function rasterIcon(source: string, onDrop: (drop: SanitizerDrop) => void) {
  if (
    source.length > 128 * 1024 ||
    !/^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(source)
  ) {
    onDrop({
      what: "icon",
      reason:
        "The plugin icon must be a normalized PNG from its folder. Reload the plugin to refresh it.",
    });
    return null;
  }
  const image = document.createElement("img");
  image.src = source;
  image.alt = "";
  image.width = 128;
  image.height = 128;
  image.draggable = false;
  image.style.cssText = "display:block;width:1em;height:1em;object-fit:contain";
  return image;
}

/** One branding renderer for the bar and settings: tile SVG, folder PNG, fallback. */
export function PluginIcon({
  pluginId,
  tileId,
  svg,
  dataUrl,
  fallback,
  className,
  style,
}: PluginIconProps) {
  const hostRef = useRef<HTMLSpanElement>(null);
  const hasFallback = fallback !== undefined;

  useEffect(() => {
    const host = hostRef.current;
    if (host === null) return;
    const root = host.shadowRoot ?? host.attachShadow({ mode: "open" });
    const drops: SanitizerDrop[] = [];
    const onDrop = (drop: SanitizerDrop) => drops.push(drop);
    const icon =
      (svg ? sanitizedIcon(svg, onDrop) : null) ??
      (dataUrl ? rasterIcon(dataUrl, onDrop) : null);
    reportMarkupDrops(pluginId, tileId, "icon", drops);
    const showFallback = () => {
      host.hidden = !hasFallback;
      root.replaceChildren(document.createElement("slot"));
    };
    if (icon === null) {
      showFallback();
      return;
    }
    host.hidden = false;
    root.replaceChildren(icon);
    const onError = () => {
      reportMarkupDrops(pluginId, tileId, "icon", [
        {
          what: "icon",
          reason:
            "The plugin icon could not be displayed. Reload the plugin to refresh it.",
        },
      ]);
      showFallback();
    };
    icon.addEventListener("error", onError);
    return () => {
      icon.removeEventListener("error", onError);
    };
  }, [pluginId, svg, dataUrl, tileId, hasFallback]);

  return (
    <span
      ref={hostRef}
      className={className}
      style={style}
      aria-hidden="true"
      hidden
    >
      {fallback}
    </span>
  );
}
