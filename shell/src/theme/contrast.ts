/**
 * Readable-text derivation for theme tokens and per-tile branding.
 *
 * The bar has exactly one automatic colour rule: a text colour that fails
 * against the surface it sits on is replaced by one that works. Everything
 * else stays the author's choice — themes, manifests and the colour pickers
 * all keep their declared values as long as those values are legible. That
 * "only fix what is broken" rule is why a single helper can serve the theme
 * layer, plugin branding and popups without special cases.
 *
 * Colours arrive as CSS strings. The small parser keeps pure logic usable in
 * tests and non-DOM callers; in the webview a CSS probe plus a one-pixel canvas
 * resolves named colours, modern colour spaces, color-mix() and var() chains
 * to sRGB before the WCAG calculation. Invalid or non-colour values still
 * yield no opinion rather than silently becoming black.
 */

/**
 * Below this ratio a declared text colour counts as broken and is replaced.
 *
 * WCAG 2.2 SC 1.4.3 requires 4.5:1 for the small text used by the bar and
 * plugin kit. The relaxed 3:1 threshold only applies to large text.
 */
export const MIN_CONTRAST = 4.5;

const LIGHT_TEXT = "#ffffff";
const DARK_TEXT = "#09060f";

type Rgb = readonly [number, number, number];

/** sRGB channels 0-255 from `#rgb`, `#rrggbb(aa)`, `rgb()` and `rgba()`. */
export function parseColor(css: string): Rgb | null {
  const value = css.trim().toLowerCase();
  const hex = /^#([0-9a-f]{3,8})$/.exec(value)?.[1];
  if (hex !== undefined) {
    const expand =
      hex.length === 3 || hex.length === 4
        ? hex.replace(/./g, (digit) => digit + digit)
        : hex;
    if (expand.length !== 6 && expand.length !== 8) return null;
    const channel = (at: number) =>
      Number.parseInt(expand.slice(at, at + 2), 16);
    return [channel(0), channel(2), channel(4)];
  }
  const parts = /^rgba?\(([^)]+)\)$/.exec(value)?.[1];
  // Percent channels need browser resolution; treating 100% as the number
  // 100 would silently turn white into a dark grey.
  if (parts === undefined || parts.includes("%")) return null;
  const numbers = parts
    .split(/[\s,/]+/)
    .filter((part) => part !== "")
    .map(Number.parseFloat);
  const [r, g, b] = numbers;
  if (r === undefined || g === undefined || b === undefined) return null;
  if (![r, g, b].every((n) => Number.isFinite(n))) return null;
  return [r, g, b];
}

/** Resolves any browser-supported CSS colour to sRGB without inventing one. */
export function resolveColor(css: string): Rgb | null {
  const direct = parseColor(css);
  if (direct !== null) return direct;
  if (typeof document === "undefined" || css.trim() === "") return null;

  const wrapper = document.createElement("span");
  const probe = document.createElement("span");
  probe.style.color = css;
  if (probe.style.color === "") return null;
  wrapper.style.position = "fixed";
  wrapper.style.visibility = "hidden";
  wrapper.style.pointerEvents = "none";
  wrapper.style.color = "rgb(1, 2, 3)";
  wrapper.append(probe);
  document.documentElement.append(wrapper);
  const inheritedA = getComputedStyle(probe).color;
  wrapper.style.color = "rgb(4, 5, 6)";
  const computed = getComputedStyle(probe).color;
  wrapper.remove();

  // Missing var() values, currentColor and inherit resolve to the parent.
  // Changing a sentinel parent exposes them instead of accepting its colour.
  if (inheritedA !== computed) return null;

  const parsed = parseColor(computed);
  if (parsed !== null) return parsed;

  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (context === null) return null;
  if (!canvasAcceptsColor(context, computed)) return null;
  context.clearRect(0, 0, 1, 1);
  context.fillRect(0, 0, 1, 1);
  const pixel = context.getImageData(0, 0, 1, 1).data;
  const [r, g, b] = pixel;
  return r === undefined || g === undefined || b === undefined
    ? null
    : [r, g, b];
}

/** Canvas keeps its old fillStyle when an engine does not support a colour. */
function canvasAcceptsColor(
  context: CanvasRenderingContext2D,
  css: string,
): boolean {
  for (const sentinel of ["#010203", "#040506"] as const) {
    context.fillStyle = sentinel;
    context.fillStyle = css;
    if (context.fillStyle !== sentinel) return true;
  }
  return false;
}

/** Canonical six-digit hex for a measurable colour, otherwise `null`. */
export function colorToHex(css: string): string | null {
  const rgb = resolveColor(css);
  if (rgb === null) return null;
  return `#${rgb.map(hexChannel).join("")}`;
}

function hexChannel(channel: number): string {
  return Math.round(Math.min(Math.max(channel, 0), 255))
    .toString(16)
    .padStart(2, "0");
}

/** CSS colour equality with a raw-string fallback for unresolved var()s. */
export function colorsEqual(a: string, b: string): boolean {
  const left = resolveColor(a);
  const right = resolveColor(b);
  if (left !== null && right !== null) {
    return channelsEqual(left, right);
  }
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

function channelsEqual(
  left: readonly number[],
  right: readonly number[],
): boolean {
  return (
    left.length === right.length &&
    left.every((channel, index) => channel === right[index])
  );
}

/** WCAG relative luminance (0 = black, 1 = white). */
export function relativeLuminance(rgb: Rgb): number {
  const linear = (channel: number) => {
    const s = Math.min(Math.max(channel, 0), 255) / 255;
    return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  };
  return (
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
  );
}

/** WCAG contrast ratio (1 = identical, 21 = black on white). */
export function contrastRatio(a: Rgb, b: Rgb): number {
  const [lo, hi] = [relativeLuminance(a), relativeLuminance(b)].sort(
    (x, y) => x - y,
  );
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05);
}

/** The better-contrasting of white and near-black on `background`. */
export function contrastText(background: string): string | null {
  return contrastTextOnAll([background]);
}

/** Lowest contrast of `text` against every supplied background. */
export function minimumContrast(
  backgrounds: readonly string[],
  text: string,
): number | null {
  const foreground = resolveColor(text);
  if (backgrounds.length === 0 || foreground === null) return null;
  const resolved: Rgb[] = [];
  for (const background of backgrounds) {
    const color = resolveColor(background);
    if (color === null) return null;
    resolved.push(color);
  }
  return Math.min(...resolved.map((color) => contrastRatio(color, foreground)));
}

/** The white/near-black foreground with the best worst-case contrast. */
export function contrastTextOnAll(
  backgrounds: readonly string[],
): string | null {
  const light = minimumContrast(backgrounds, LIGHT_TEXT);
  const dark = minimumContrast(backgrounds, DARK_TEXT);
  if (light === null || dark === null) return null;
  return light >= dark ? LIGHT_TEXT : DARK_TEXT;
}

/**
 * `declared` when it is legible on `background`, otherwise a colour that is.
 * Returns `null` when there is nothing to say — unparseable input, or a
 * declared colour that already works — so callers can leave the token alone.
 */
export function readableTextOn(
  background: string | undefined,
  declared: string | undefined,
): string | null {
  return background === undefined
    ? null
    : readableTextOnAll([background], declared);
}

/**
 * Repairs one foreground against every endpoint it appears on. A declared
 * colour is kept when it already passes, or when neither neutral candidate
 * improves an unavoidable low-contrast combination.
 */
export function readableTextOnAll(
  backgrounds: readonly string[],
  declared: string | undefined,
): string | null {
  const declaredRatio =
    declared === undefined ? null : minimumContrast(backgrounds, declared);
  if (declaredRatio !== null && declaredRatio >= MIN_CONTRAST) return null;

  const fixed = contrastTextOnAll(backgrounds);
  if (fixed === null) return null;
  const fixedRatio = minimumContrast(backgrounds, fixed);
  if (
    fixedRatio === null ||
    (declaredRatio !== null && declaredRatio >= fixedRatio) ||
    (declared !== undefined && colorsEqual(fixed, declared))
  ) {
    return null;
  }
  return fixed;
}
