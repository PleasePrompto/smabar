import type { SanitizerDrop } from "./sanitize";

export const FLYOUT_WIDTH_ATTR = "data-sb-flyout-width";
export const DEFAULT_FLYOUT_WIDTH = "21.25rem";

/** Read only the sanitized fragment, before it moves into the shadow root. */
export function readFlyoutWidth(
  markup: DocumentFragment,
  target: string,
  onDrop: (drop: SanitizerDrop) => void,
): string {
  const requests = markup.querySelectorAll(`[${FLYOUT_WIDTH_ATTR}]`);
  if (requests.length === 0) return DEFAULT_FLYOUT_WIDTH;
  const element = requests[0];
  const value = element?.getAttribute(FLYOUT_WIDTH_ATTR)?.trim() ?? "";
  const pixels = /^[0-9]+$/.test(value) ? Number(value) : 0;
  const hasSiblingText = [...markup.childNodes].some(
    (node) =>
      node.nodeType === Node.TEXT_NODE &&
      (node.textContent ?? "").trim() !== "",
  );
  if (
    target === "flyout" &&
    requests.length === 1 &&
    markup.children.length === 1 &&
    element === markup.firstElementChild &&
    !hasSiblingText
  ) {
    if (value === "wide") return "42.5rem";
    if (pixels >= 1 && pixels <= 16_384) return `${String(pixels)}px`;
  }
  for (const request of requests) {
    request.removeAttribute(FLYOUT_WIDTH_ATTR);
    onDrop({
      what: `${request.localName}[${FLYOUT_WIDTH_ATTR}]`,
      reason:
        'Use data-sb-flyout-width="wide" or a whole CSS-pixel count from 1 to 16384 on the single outer element of flyout/hover HTML. Invalid or ambiguous requests use the default width.',
    });
  }
  return DEFAULT_FLYOUT_WIDTH;
}
