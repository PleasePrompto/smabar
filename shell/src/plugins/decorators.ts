/** Post-sanitize UI-kit conventions that need shell-owned DOM. */

import { SUPPRESS_DELEGATED_CLICK_ATTR } from "./behaviour/delegate";

export { activateTab, enhanceTabs } from "./tabs";
export { enhanceMarquees, measureMarquee } from "./marquee";

let tooltipDescriptionSequence = 0;

/**
 * Moves plugin `title` tooltips onto the shell's themed tooltip layer.
 *
 * A native `title` renders the webview's GTK tooltip — light system palette,
 * no theme, wrong font — so the attribute is REMOVED and its text handed to
 * `data-sb-tooltip` (components/overlay/Tooltip.tsx). Existing names keep the
 * title as an accessible description; unnamed elements keep it as aria-label.
 * Idempotent: a re-render replaces the whole subtree anyway.
 */
export function enhanceTooltips(root: ParentNode): void {
  for (const element of root.querySelectorAll<HTMLElement>("[title]")) {
    // A frame's title is its required accessible name. It also cannot drive
    // the parent document's pointer-based themed tooltip once entered.
    if (element instanceof HTMLIFrameElement) continue;
    const text = element.getAttribute("title") ?? "";
    element.removeAttribute("title");
    if (text.trim() === "") continue;
    element.dataset.sbTooltip = text;
    const hasName =
      (element.getAttribute("aria-label")?.trim() ?? "") !== "" ||
      (element.getAttribute("aria-labelledby")?.trim() ?? "") !== "" ||
      element.textContent.trim() !== "" ||
      (element instanceof HTMLImageElement && element.alt.trim() !== "") ||
      ((element instanceof HTMLButtonElement ||
        element instanceof HTMLInputElement ||
        element instanceof HTMLSelectElement ||
        element instanceof HTMLTextAreaElement) &&
        (element.labels?.length ?? 0) > 0);
    if (!hasName) {
      element.setAttribute("aria-label", text);
      continue;
    }

    let descriptionId: string;
    do {
      tooltipDescriptionSequence += 1;
      descriptionId = `sb-tooltip-description-${String(tooltipDescriptionSequence)}`;
    } while (root.querySelector(`#${descriptionId}`) !== null);
    const description = element.ownerDocument.createElement("span");
    description.id = descriptionId;
    description.className = "sb-sr-only";
    description.textContent = text;
    const descriptions = new Set(
      element.getAttribute("aria-describedby")?.match(/\S+/g) ?? [],
    );
    descriptions.add(descriptionId);
    element.setAttribute("aria-describedby", [...descriptions].join(" "));
    if (root instanceof Document) root.body.append(description);
    else root.append(description);
  }
}

/** Renders data-badge values as shell-owned accent badges (empty = dot). */
export function enhanceBadges(root: ParentNode): void {
  for (const element of root.querySelectorAll<HTMLElement>("[data-badge]")) {
    const badge = element.ownerDocument.createElement("span");
    const value = element.dataset.badge ?? "";
    badge.className = `sb-data-badge${value === "" ? " sb-data-badge-dot" : ""}`;
    badge.setAttribute("aria-hidden", "true");
    badge.textContent = value;
    element.classList.add("sb-badge-anchor");
    element.appendChild(badge);
  }
}

const CAROUSEL_MIN_INTERVAL_MS = 1_500;

/** Effective auto-advance interval; absent/broken values mean manual only. */
export function carouselInterval(raw: string | undefined): number | null {
  if (raw === undefined || raw.trim() === "") return null;
  const value = Number(raw);
  if (!Number.isFinite(value) || value <= 0) return null;
  return Math.max(CAROUSEL_MIN_INTERVAL_MS, Math.round(value));
}

function carouselSlides(element: HTMLElement): HTMLElement[] {
  return Array.from(element.children).filter(
    (child): child is HTMLElement => child instanceof HTMLElement,
  );
}

function slideLeft(
  carousel: HTMLElement,
  slide: HTMLElement,
  index: number,
): number {
  const measured =
    slide.offsetParent === carousel || slide.offsetParent === null
      ? slide.offsetLeft
      : slide.offsetParent === carousel.offsetParent
        ? slide.offsetLeft - carousel.offsetLeft
        : slide.getBoundingClientRect().left -
          carousel.getBoundingClientRect().left +
          carousel.scrollLeft;
  return measured !== 0 || index === 0
    ? measured
    : index * carousel.clientWidth;
}

function nearestSlide(carousel: HTMLElement, slides: HTMLElement[]): number {
  let nearest = 0;
  let distance = Infinity;
  slides.forEach((slide, index) => {
    const next = Math.abs(
      slideLeft(carousel, slide, index) - carousel.scrollLeft,
    );
    if (next < distance) {
      nearest = index;
      distance = next;
    }
  });
  return nearest;
}

function scrollToSlide(
  carousel: HTMLElement,
  slides: HTMLElement[],
  index: number,
  smooth: boolean,
): void {
  if (slides.length === 0) return;
  const wrapped = ((index % slides.length) + slides.length) % slides.length;
  const slide = slides[wrapped];
  if (slide === undefined) return;
  carousel.scrollTo({
    left: slideLeft(carousel, slide, wrapped),
    behavior: smooth ? "smooth" : "auto",
  });
}

/**
 * Turns data-carousel containers into scroll-snap sliders (every child is
 * one full-width slide). Pointer dragging uses capture so mouse, touch and
 * stylus all finish reliably; arrows are the keyboard alternative.
 */
export function enhanceCarousels(root: ParentNode): () => void {
  const cleanups: (() => void)[] = [];
  const reducedMotion =
    typeof matchMedia === "function"
      ? matchMedia("(prefers-reduced-motion: reduce)")
      : null;
  const prefersReducedMotion = () => reducedMotion?.matches === true;
  for (const element of root.querySelectorAll<HTMLElement>("[data-carousel]")) {
    element.classList.add("sb-carousel");
    if (element.tabIndex < 0) element.tabIndex = 0;
    const slides = carouselSlides(element);
    const paused = new Set<string>();
    let drag:
      | { pointerId: number; startX: number; startLeft: number; moved: boolean }
      | undefined;
    let suppressClick = false;
    let suppressTimer = 0;

    const onPointerPosition = (event: PointerEvent) => {
      if (!paused.has("pointer")) return;
      const rect = element.getBoundingClientRect();
      const inside =
        event.clientX >= rect.left &&
        event.clientX <= rect.right &&
        event.clientY >= rect.top &&
        event.clientY <= rect.bottom;
      if (inside) paused.add("pointer");
      else paused.delete("pointer");
    };
    const onPointerOver = () => paused.add("pointer");
    const onDocumentPointerOut = (event: PointerEvent) => {
      if (event.relatedTarget === null) paused.delete("pointer");
    };
    const onFocusIn = () => paused.add("focus");
    const onFocusOut = (event: FocusEvent) => {
      const next = event.relatedTarget;
      if (!(next instanceof Node) || !element.contains(next)) {
        paused.delete("focus");
      }
    };
    const onPointerDown = (event: PointerEvent) => {
      if (event.pointerType === "mouse" && event.button !== 0) return;
      drag = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startLeft: element.scrollLeft,
        moved: false,
      };
      paused.add("drag");
      element.setPointerCapture(event.pointerId);
    };
    const onPointerMove = (event: PointerEvent) => {
      if (event.pointerId !== drag?.pointerId) return;
      const distance = event.clientX - drag.startX;
      if (Math.abs(distance) > 4) {
        drag.moved = true;
        element.toggleAttribute("data-dragging", true);
        if (event.cancelable) event.preventDefault();
      }
      element.scrollLeft = drag.startLeft - distance;
    };
    const finishDrag = (event: PointerEvent) => {
      if (event.pointerId !== drag?.pointerId) return;
      const moved = drag.moved;
      drag = undefined;
      if (element.hasPointerCapture(event.pointerId)) {
        element.releasePointerCapture(event.pointerId);
      }
      paused.delete("drag");
      element.removeAttribute("data-dragging");
      if (!moved) return;
      scrollToSlide(
        element,
        slides,
        nearestSlide(element, slides),
        !prefersReducedMotion(),
      );
      suppressClick = true;
      element.setAttribute(SUPPRESS_DELEGATED_CLICK_ATTR, "");
      window.clearTimeout(suppressTimer);
      suppressTimer = window.setTimeout(() => {
        suppressClick = false;
        element.removeAttribute(SUPPRESS_DELEGATED_CLICK_ATTR);
      });
    };
    const onClick = (event: MouseEvent) => {
      if (!suppressClick) return;
      suppressClick = false;
      element.removeAttribute(SUPPRESS_DELEGATED_CLICK_ATTR);
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.target !== element) return;
      const step =
        event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
      if (step === 0 || slides.length < 2) return;
      event.preventDefault();
      scrollToSlide(
        element,
        slides,
        nearestSlide(element, slides) + step,
        !prefersReducedMotion(),
      );
    };

    element.addEventListener("pointerover", onPointerOver);
    element.ownerDocument.addEventListener("pointerover", onPointerPosition);
    element.ownerDocument.addEventListener("pointermove", onPointerPosition);
    element.ownerDocument.addEventListener("pointerout", onDocumentPointerOut);
    element.addEventListener("focusin", onFocusIn);
    element.addEventListener("focusout", onFocusOut);
    element.addEventListener("pointerdown", onPointerDown);
    element.addEventListener("pointermove", onPointerMove);
    element.addEventListener("pointerup", finishDrag);
    element.addEventListener("pointercancel", finishDrag);
    element.addEventListener("lostpointercapture", finishDrag);
    element.addEventListener("click", onClick, true);
    element.addEventListener("keydown", onKeyDown);

    const interval = carouselInterval(element.dataset.carouselInterval);
    const timer =
      interval === null || slides.length < 2
        ? 0
        : window.setInterval(() => {
            if (paused.size > 0 || prefersReducedMotion()) return;
            scrollToSlide(
              element,
              slides,
              nearestSlide(element, slides) + 1,
              true,
            );
          }, interval);
    cleanups.push(() => {
      window.clearInterval(timer);
      window.clearTimeout(suppressTimer);
      element.removeAttribute(SUPPRESS_DELEGATED_CLICK_ATTR);
      element.removeEventListener("pointerover", onPointerOver);
      element.ownerDocument.removeEventListener(
        "pointerover",
        onPointerPosition,
      );
      element.ownerDocument.removeEventListener(
        "pointermove",
        onPointerPosition,
      );
      element.ownerDocument.removeEventListener(
        "pointerout",
        onDocumentPointerOut,
      );
      element.removeEventListener("focusin", onFocusIn);
      element.removeEventListener("focusout", onFocusOut);
      element.removeEventListener("pointerdown", onPointerDown);
      element.removeEventListener("pointermove", onPointerMove);
      element.removeEventListener("pointerup", finishDrag);
      element.removeEventListener("pointercancel", finishDrag);
      element.removeEventListener("lostpointercapture", finishDrag);
      element.removeEventListener("click", onClick, true);
      element.removeEventListener("keydown", onKeyDown);
    });
  }
  return () => {
    cleanups.forEach((cleanup) => {
      cleanup();
    });
  };
}

const ROTATOR_MIN_INTERVAL_MS = 1_500;
const ROTATOR_DEFAULT_INTERVAL_MS = 4_000;
/** Slightly above the --sb-dur-slow transition so the leaver settles. */
const ROTATOR_SETTLE_MS = 400;

interface RotatorShift {
  exit: [string, string];
  enter: [string, string];
}

/** Exit/enter offsets: content rolls AWAY toward the given direction, the
 *  next view enters from the opposite side. Unknown values mean "up". */
export function rotatorShift(raw: string | undefined): RotatorShift {
  switch (raw) {
    case "down":
      return { exit: ["0", "100%"], enter: ["0", "-100%"] };
    case "left":
      return { exit: ["-100%", "0"], enter: ["100%", "0"] };
    case "right":
      return { exit: ["100%", "0"], enter: ["-100%", "0"] };
    default:
      return { exit: ["0", "-100%"], enter: ["0", "100%"] };
  }
}

/** Effective rotation interval; the attribute is optional. */
export function rotatorInterval(raw: string | undefined): number {
  if (raw === undefined || raw.trim() === "")
    return ROTATOR_DEFAULT_INTERVAL_MS;
  const value = Number(raw);
  if (!Number.isFinite(value) || value <= 0) return ROTATOR_DEFAULT_INTERVAL_MS;
  return Math.max(ROTATOR_MIN_INTERVAL_MS, Math.round(value));
}

/**
 * Turns data-rotator containers into rolling view stacks: every direct
 * child is one view, all stacked in the same grid cell (the container
 * sizes to the tallest view). The shell shows one view at a time and rolls
 * to the next every data-rotator-interval ms in the direction given by
 * data-rotator="up|down|left|right", pausing while hovered; under
 * prefers-reduced-motion views switch instantly. Returns a cleanup for the
 * timers owned by this render.
 */
export function enhanceRotators(root: ParentNode): () => void {
  const cleanups: (() => void)[] = [];
  const reducedMotion =
    typeof matchMedia === "function" &&
    matchMedia("(prefers-reduced-motion: reduce)").matches;
  for (const element of root.querySelectorAll<HTMLElement>("[data-rotator]")) {
    element.classList.add("sb-rotator");
    const items = [...element.children].filter(
      (child): child is HTMLElement => child instanceof HTMLElement,
    );
    const first = items[0];
    if (first === undefined) continue;
    first.classList.add("sb-rotator-active");
    if (items.length < 2) continue;

    // Authored inline transforms already overrode kit motion; leave them intact.
    const motionItems = new Set(
      items.filter((item) => item.style.transform === ""),
    );
    // Motion must not inherit: changing custom properties here makes WebKit retain
    // fresh descendant styles in its matched-declaration cache on every rotation.
    const setShift = (item: HTMLElement, [x, y]: [string, string]): void => {
      if (motionItems.has(item)) item.style.transform = `translate(${x}, ${y})`;
    };
    const clearShift = (item: HTMLElement): void => {
      if (motionItems.has(item)) item.style.removeProperty("transform");
    };

    const shift = rotatorShift(element.dataset.rotator);
    let paused = false;
    const pause = () => {
      paused = true;
    };
    const resume = () => {
      paused = false;
    };
    element.addEventListener("mouseenter", pause);
    element.addEventListener("mouseleave", resume);

    let index = 0;
    let settle = 0;
    const timer = window.setInterval(() => {
      if (paused) return;
      const current = items[index];
      index = (index + 1) % items.length;
      const next = items[index];
      if (current === undefined || next === undefined) return;
      window.clearTimeout(settle);
      // A fast interval may advance while the previous leaver is mid-flight.
      for (const item of items) {
        if (item !== current && item !== next) {
          item.classList.remove("sb-rotator-leaving", "sb-rotator-active");
          clearShift(item);
        }
      }
      if (reducedMotion) {
        current.classList.remove("sb-rotator-active");
        next.classList.add("sb-rotator-active");
        return;
      }
      // The leaver rolls out toward the direction…
      setShift(current, shift.exit);
      current.classList.replace("sb-rotator-active", "sb-rotator-leaving");
      // …while the next view starts on the opposite side (positioned
      // instantly — the base state has no transition), then slides in.
      setShift(next, shift.enter);
      void next.offsetWidth; // commit the start position before transitioning
      next.classList.add("sb-rotator-active");
      clearShift(next);
      settle = window.setTimeout(() => {
        current.classList.remove("sb-rotator-leaving");
        clearShift(current);
      }, ROTATOR_SETTLE_MS);
    }, rotatorInterval(element.dataset.rotatorInterval));

    cleanups.push(() => {
      window.clearInterval(timer);
      window.clearTimeout(settle);
      element.removeEventListener("mouseenter", pause);
      element.removeEventListener("mouseleave", resume);
    });
  }
  return () => {
    cleanups.forEach((cleanup) => {
      cleanup();
    });
  };
}
