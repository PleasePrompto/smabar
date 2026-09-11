/**
 * Which parts of the bar can be screenshotted, and how to make a scrollable
 * one fit into a single picture.
 *
 * A tile flyout clips its content at a scroll edge, so a plain snapshot of
 * the viewport would cut a long list in half. Before the core takes the
 * picture we therefore lift that clipping — and only if the subject then
 * reaches past the viewport, anchor it into the document flow so WebKit's
 * FullDocument snapshot grows to cover it.
 */

/** A rectangle in CSS pixels, mirroring `smabar_core::platform::Rect`. */
export interface CaptureRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface StagedCapture {
  rect: CaptureRect;
  /** Clipping was lifted, so the shot shows more than a user sees. */
  expanded: boolean;
  /** Something still clips even after staging: the shot is incomplete. */
  clipped: boolean;
  /** Undoes every change this staging made. Always call it. */
  release: () => void;
}

/** Every element that carries a stable capture identity, name → element. */
export function collectTargets(): Map<string, HTMLElement> {
  const targets = new Map<string, HTMLElement>();
  const dock = document.querySelector<HTMLElement>("[data-bar-dock]");
  if (dock !== null) targets.set("bar", dock);
  for (const el of document.querySelectorAll<HTMLElement>("[data-capture]")) {
    const name = el.dataset.capture;
    if (name !== undefined && name !== "" && !targets.has(name)) {
      targets.set(name, el);
    }
  }
  for (const el of document.querySelectorAll<HTMLElement>("[data-tile-id]")) {
    // The tile and its shadow host share the id; the tile is the outer one.
    if (el.parentElement?.closest("[data-tile-id]") != null) continue;
    const id = el.dataset.tileId;
    if (id !== undefined && id !== "") targets.set(id, el);
  }
  for (const el of document.querySelectorAll<HTMLElement>(
    "[data-shortcut-id]",
  )) {
    const id = el.dataset.shortcutId;
    if (id !== undefined && id !== "") targets.set(`shortcut:${id}`, el);
  }
  return targets;
}

interface StylePatch {
  el: HTMLElement;
  prop: string;
  prev: string;
  priority: string;
}

function patch(
  patches: StylePatch[],
  el: HTMLElement,
  prop: string,
  value: string,
): void {
  patches.push({
    el,
    prop,
    prev: el.style.getPropertyValue(prop),
    priority: el.style.getPropertyPriority(prop),
  });
  el.style.setProperty(prop, value, "important");
}

/** Depth-first over the subtree, descending into open shadow roots — plugin
 *  markup lives in one, and that is exactly where long lists are. */
function* descend(root: ParentNode): Generator<HTMLElement> {
  for (const el of root.querySelectorAll<HTMLElement>("*")) {
    yield el;
    if (el.shadowRoot !== null) yield* descend(el.shadowRoot);
  }
}

/**
 * Whether this element HIDES part of its content.
 *
 * Overflowing is not the same as clipping: an element with `overflow:
 * visible` spills its content into view and loses nothing. That distinction
 * matters twice — an unclipped element needs no staging, and after staging
 * every freed element still reports `scrollHeight > clientHeight` while no
 * longer cutting anything, so testing the overflow alone would report every
 * successful capture as incomplete.
 *
 * 1px of slack: sub-pixel layout exceeds the client box by a hair on
 * elements that clip nothing.
 */
function clippedAxes(el: HTMLElement): {
  vertical: boolean;
  horizontal: boolean;
} {
  const style = getComputedStyle(el);
  return {
    vertical:
      el.scrollHeight > el.clientHeight + 1 && style.overflowY !== "visible",
    horizontal:
      el.scrollWidth > el.clientWidth + 1 && style.overflowX !== "visible",
  };
}

function clips(el: HTMLElement): boolean {
  const axes = clippedAxes(el);
  return axes.vertical || axes.horizontal;
}

function measure(el: HTMLElement): CaptureRect {
  const r = el.getBoundingClientRect();
  return {
    x: Math.floor(r.left + window.scrollX),
    y: Math.floor(r.top + window.scrollY),
    w: Math.ceil(r.width),
    h: Math.ceil(r.height),
  };
}

/**
 * Prepares `subject` to be photographed whole and reports its box.
 * The caller MUST call `release()` afterwards, success or not.
 */
export function stageCapture(subject: HTMLElement): StagedCapture {
  const patches: StylePatch[] = [];
  const release = (): void => {
    for (const { el, prop, prev, priority } of patches.reverse()) {
      if (prev === "") el.style.removeProperty(prop);
      else el.style.setProperty(prop, prev, priority);
    }
  };

  // Innermost first: unclipping a child grows it, and only then does its
  // parent report as scrolling. Outermost-first would clear the parent while
  // the child still fits, and the parent would never be revisited.
  let expanded = false;
  for (const el of [...descend(subject)].reverse().concat(subject)) {
    const axes = clippedAxes(el);
    if (!axes.vertical && !axes.horizontal) continue;
    expanded = true;
    // Free only the axis that clips. A chart that merely scrolls sideways
    // keeps its height: releasing it would collapse the bars to nothing and
    // photograph a broken chart that renders fine on screen.
    // Longhands, not the `overflow` shorthand: these are the properties read
    // back above, and touching exactly them keeps the restore exact.
    if (axes.vertical) {
      patch(patches, el, "max-height", "none");
      // A fixed `height` ignores max-height, so release that too: a scroll
      // container's height is a viewport budget rather than part of the subject.
      patch(patches, el, "height", "auto");
      patch(patches, el, "overflow-y", "visible");
    }
    if (axes.horizontal) {
      patch(patches, el, "max-width", "none");
      patch(patches, el, "overflow-x", "visible");
    }
  }

  // Anything still scrolling could not be freed by the rules above; say so
  // rather than returning a silently cropped picture.
  const clipped = [subject, ...descend(subject)].some(clips);

  let rect = measure(subject);
  const fitsViewport =
    rect.x >= 0 &&
    rect.y >= 0 &&
    rect.x + rect.w <= window.innerWidth &&
    rect.y + rect.h <= window.innerHeight;
  if (!fitsViewport) {
    // Let the document grow past the viewport (WebKit's FullDocument
    // snapshot follows the document box, not the window), then park the
    // subject at its origin so the whole of it is inside that box.
    for (const el of [document.documentElement, document.body]) {
      patch(patches, el, "overflow-x", "visible");
      patch(patches, el, "overflow-y", "visible");
      patch(patches, el, "height", "auto");
    }
    patch(patches, subject, "position", "absolute");
    patch(patches, subject, "inset", "auto");
    patch(patches, subject, "left", "0");
    patch(patches, subject, "top", "0");
    patch(patches, subject, "margin", "0");
    patch(patches, subject, "transform", "none");
    // Pin the width it had while laid out normally: a percentage width would
    // otherwise resolve against a different container and reflow the shot.
    patch(patches, subject, "width", `${String(rect.w)}px`);
    rect = measure(subject);
  }

  return { rect, expanded, clipped, release };
}
