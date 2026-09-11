// @vitest-environment happy-dom
import { afterEach, expect, test, vi } from "vitest";

import { enhanceCarousels } from "./decorators";
import { behaviour, installKitBehaviour } from "./behaviour/delegate";

let delegatedCarouselClicks = 0;
behaviour("click", "[data-carousel-test-action]", () => {
  delegatedCarouselClicks += 1;
});

afterEach(() => {
  delegatedCarouselClicks = 0;
  vi.useRealTimers();
  vi.restoreAllMocks();
});

function fixture(interval?: string): {
  root: HTMLDivElement;
  carousel: HTMLElement;
  scrollTo: ReturnType<typeof vi.fn>;
} {
  const root = document.createElement("div");
  root.innerHTML = `<div data-carousel${interval ? ` data-carousel-interval="${interval}"` : ""}>
    <div>One</div><div>Two</div><div>Three</div>
  </div>`;
  const carousel = root.querySelector<HTMLElement>("[data-carousel]");
  if (carousel === null) throw new Error("carousel missing");
  Object.defineProperty(carousel, "clientWidth", { value: 100 });
  Object.defineProperty(carousel, "offsetLeft", { value: 0 });
  [...carousel.children].forEach((slide, index) => {
    Object.defineProperty(slide, "offsetLeft", { value: index * 110 });
    Object.defineProperty(slide, "offsetParent", { value: carousel });
  });
  const scrollTo = vi.fn((options: ScrollToOptions) => {
    carousel.scrollLeft = options.left ?? carousel.scrollLeft;
  });
  Object.defineProperty(carousel, "scrollTo", { value: scrollTo });
  Object.defineProperty(carousel, "setPointerCapture", { value: vi.fn() });
  Object.defineProperty(carousel, "hasPointerCapture", {
    value: () => true,
  });
  Object.defineProperty(carousel, "releasePointerCapture", { value: vi.fn() });
  return { root, carousel, scrollTo };
}

function pointer(
  element: Element,
  type: string,
  clientX: number,
): PointerEvent {
  const event = new PointerEvent(type, {
    pointerId: 7,
    pointerType: "touch",
    clientX,
    button: 0,
    bubbles: true,
    cancelable: true,
  });
  element.dispatchEvent(event);
  return event;
}

test("carousel pointer drag captures, scrolls, snaps, and suppresses the drag click", () => {
  const { root, carousel, scrollTo } = fixture();
  const cleanup = enhanceCarousels(root);

  pointer(carousel, "pointerdown", 100);
  const move = pointer(carousel, "pointermove", -20);
  expect(move.defaultPrevented).toBe(true);
  expect(carousel.scrollLeft).toBe(120);
  expect(carousel.hasAttribute("data-dragging")).toBe(true);
  pointer(carousel, "pointerup", -20);
  expect(scrollTo).toHaveBeenLastCalledWith({ left: 110, behavior: "smooth" });
  expect(carousel.hasAttribute("data-dragging")).toBe(false);

  const click = new MouseEvent("click", {
    bubbles: true,
    cancelable: true,
  });
  carousel.dispatchEvent(click);
  expect(click.defaultPrevented).toBe(true);
  cleanup();
});

test("a drag click is suppressed before delegated shell behaviours run", () => {
  const { root, carousel } = fixture();
  const action = document.createElement("button");
  action.setAttribute("data-carousel-test-action", "");
  carousel.firstElementChild?.append(action);
  document.body.append(root);
  const stopBehaviour = installKitBehaviour();
  const cleanup = enhanceCarousels(root);

  try {
    pointer(carousel, "pointerdown", 100);
    pointer(carousel, "pointermove", 20);
    pointer(carousel, "pointerup", 20);
    const click = new MouseEvent("click", {
      bubbles: true,
      cancelable: true,
      composed: true,
    });
    action.dispatchEvent(click);

    expect(delegatedCarouselClicks).toBe(0);
    expect(click.defaultPrevented).toBe(true);
  } finally {
    cleanup();
    stopBehaviour();
    root.remove();
  }
});

test("lost pointer capture clears drag state and resumes autoplay", () => {
  vi.useFakeTimers();
  const { root, carousel, scrollTo } = fixture("1500");
  const cleanup = enhanceCarousels(root);

  pointer(carousel, "pointerdown", 100);
  pointer(carousel, "pointermove", 40);
  expect(carousel.hasAttribute("data-dragging")).toBe(true);
  carousel.dispatchEvent(
    new PointerEvent("lostpointercapture", {
      pointerId: 7,
      pointerType: "touch",
    }),
  );
  expect(carousel.hasAttribute("data-dragging")).toBe(false);

  scrollTo.mockClear();
  vi.advanceTimersByTime(1_500);
  expect(scrollTo).toHaveBeenCalledOnce();
  cleanup();
});

test("ArrowLeft and ArrowRight expose a gap-aware keyboard alternative", () => {
  const { root, carousel, scrollTo } = fixture();
  const cleanup = enhanceCarousels(root);
  carousel.focus();

  const right = new KeyboardEvent("keydown", {
    key: "ArrowRight",
    bubbles: true,
    cancelable: true,
  });
  carousel.dispatchEvent(right);
  expect(right.defaultPrevented).toBe(true);
  expect(scrollTo).toHaveBeenLastCalledWith({ left: 110, behavior: "smooth" });
  carousel.scrollLeft = 110;
  carousel.dispatchEvent(
    new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }),
  );
  expect(scrollTo).toHaveBeenLastCalledWith({ left: 0, behavior: "smooth" });
  cleanup();
});

test("autoplay wraps, pauses for pointer/focus, and cleanup removes it", () => {
  vi.useFakeTimers();
  const { root, carousel, scrollTo } = fixture("1500");
  const cleanup = enhanceCarousels(root);

  carousel.dispatchEvent(
    new PointerEvent("pointerover", {
      bubbles: true,
      clientX: 0,
      clientY: 0,
    }),
  );
  vi.advanceTimersByTime(1_500);
  expect(scrollTo).not.toHaveBeenCalled();
  document.body.dispatchEvent(
    new PointerEvent("pointerover", {
      bubbles: true,
      clientX: 10,
      clientY: 10,
    }),
  );
  vi.advanceTimersByTime(1_500);
  expect(scrollTo).toHaveBeenLastCalledWith({ left: 110, behavior: "smooth" });

  carousel.scrollLeft = 220;
  vi.advanceTimersByTime(1_500);
  expect(scrollTo).toHaveBeenLastCalledWith({ left: 0, behavior: "smooth" });
  cleanup();
  scrollTo.mockClear();
  vi.advanceTimersByTime(10_000);
  expect(scrollTo).not.toHaveBeenCalled();
});

test("leaving the document clears the coordinate-based autoplay pause", () => {
  vi.useFakeTimers();
  const { root, carousel, scrollTo } = fixture("1500");
  const cleanup = enhanceCarousels(root);

  carousel.dispatchEvent(
    new PointerEvent("pointerover", {
      bubbles: true,
      clientX: 0,
      clientY: 0,
    }),
  );
  document.dispatchEvent(
    new PointerEvent("pointerout", { bubbles: true, relatedTarget: null }),
  );
  vi.advanceTimersByTime(1_500);
  expect(scrollTo).toHaveBeenCalledOnce();
  cleanup();
});

test("unrelated pointer moves do not measure inactive carousels", () => {
  const first = fixture();
  const second = fixture();
  const firstRect = vi.spyOn(first.carousel, "getBoundingClientRect");
  const secondRect = vi.spyOn(second.carousel, "getBoundingClientRect");
  const cleanupFirst = enhanceCarousels(first.root);
  const cleanupSecond = enhanceCarousels(second.root);

  document.body.dispatchEvent(
    new PointerEvent("pointermove", {
      bubbles: true,
      clientX: 10,
      clientY: 10,
    }),
  );
  expect(firstRect).not.toHaveBeenCalled();
  expect(secondRect).not.toHaveBeenCalled();

  first.carousel.dispatchEvent(
    new PointerEvent("pointerover", { bubbles: true }),
  );
  document.body.dispatchEvent(
    new PointerEvent("pointermove", {
      bubbles: true,
      clientX: 10,
      clientY: 10,
    }),
  );
  expect(firstRect).toHaveBeenCalledOnce();
  expect(secondRect).not.toHaveBeenCalled();

  cleanupFirst();
  cleanupSecond();
});

test("reduced motion disables autoplay and makes manual movement instant", () => {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, "matchMedia");
  Object.defineProperty(globalThis, "matchMedia", {
    configurable: true,
    value: () => ({ matches: true }),
  });
  vi.useFakeTimers();
  try {
    const { root, carousel, scrollTo } = fixture("1500");
    const cleanup = enhanceCarousels(root);
    vi.advanceTimersByTime(5_000);
    expect(scrollTo).not.toHaveBeenCalled();
    carousel.focus();
    carousel.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }),
    );
    expect(scrollTo).toHaveBeenLastCalledWith({ left: 110, behavior: "auto" });
    cleanup();
  } finally {
    if (descriptor === undefined)
      Reflect.deleteProperty(globalThis, "matchMedia");
    else Object.defineProperty(globalThis, "matchMedia", descriptor);
  }
});

test("enabling reduced motion stops autoplay and smooth manual movement immediately", () => {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, "matchMedia");
  let reduce = false;
  Object.defineProperty(globalThis, "matchMedia", {
    configurable: true,
    value: () => ({
      get matches() {
        return reduce;
      },
    }),
  });
  vi.useFakeTimers();
  try {
    const { root, carousel, scrollTo } = fixture("1500");
    const cleanup = enhanceCarousels(root);
    vi.advanceTimersByTime(1_500);
    expect(scrollTo).toHaveBeenLastCalledWith({
      left: 110,
      behavior: "smooth",
    });

    scrollTo.mockClear();
    reduce = true;
    vi.advanceTimersByTime(1_500);
    expect(scrollTo).not.toHaveBeenCalled();
    carousel.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "ArrowRight",
        bubbles: true,
        cancelable: true,
      }),
    );
    expect(scrollTo).toHaveBeenLastCalledWith({ left: 220, behavior: "auto" });
    cleanup();
  } finally {
    if (descriptor === undefined)
      Reflect.deleteProperty(globalThis, "matchMedia");
    else Object.defineProperty(globalThis, "matchMedia", descriptor);
  }
});
