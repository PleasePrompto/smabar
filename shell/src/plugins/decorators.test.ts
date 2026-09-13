// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";

import {
  activateTab,
  carouselInterval,
  enhanceCarousels,
  enhanceMarquees,
  enhanceRotators,
  enhanceTabs,
  measureMarquee,
  rotatorInterval,
  rotatorShift,
} from "./decorators";
import { container } from "./pluginTestDom";

test("marquee enhancer wraps content and cleans up observers", () => {
  const root = container("<span data-marquee>Long status text</span>");
  const element = root.querySelector<HTMLElement>("[data-marquee]");
  expect(element).not.toBeNull();
  if (element === null) return;
  const original = globalThis.ResizeObserver;
  let disconnected = false;
  let observed = 0;
  class TestResizeObserver {
    observe(): void {
      observed += 1;
    }
    disconnect(): void {
      disconnected = true;
    }
  }
  Object.assign(globalThis, { ResizeObserver: TestResizeObserver });
  const contentBefore = element.textContent;
  const cleanup = enhanceMarquees(root);
  const content = element.querySelector<HTMLElement>(".sb-marquee-content");
  expect(content).not.toBeNull();
  expect(element.textContent).toBe(contentBefore);
  expect(observed).toBe(2);
  cleanup();
  expect(disconnected).toBe(true);
  Object.assign(globalThis, { ResizeObserver: original });
});

test("marquee marks overflow and leaves fitting text still", () => {
  const overflowing = container("<span data-marquee>Long text</span>");
  const fitting = container("<span data-marquee>Short</span>");
  for (const [root, width, scrollWidth] of [
    [overflowing, 50, 120],
    [fitting, 80, 80],
  ] as const) {
    const cleanup = enhanceMarquees(root);
    const element = root.querySelector<HTMLElement>("[data-marquee]");
    const content = root.querySelector<HTMLElement>(".sb-marquee-content");
    if (element === null || content === null) continue;
    Object.defineProperty(element, "clientWidth", { value: width });
    Object.defineProperty(content, "scrollWidth", { value: scrollWidth });
    measureMarquee(element, content);
    cleanup();
  }
  expect(
    overflowing
      .querySelector("[data-marquee]")
      ?.hasAttribute("data-marquee-overflow"),
  ).toBe(true);
  expect(
    fitting
      .querySelector("[data-marquee]")
      ?.hasAttribute("data-marquee-overflow"),
  ).toBe(false);
});

test("marquees hidden by closed details initialize only when revealed", () => {
  const root = container(
    '<details><summary><span id="summary" data-marquee>Summary</span></summary>' +
      '<span id="body" data-marquee>Deferred body</span></details>',
  );
  const details = root.querySelector("details");
  const original = globalThis.ResizeObserver;
  let observed = 0;
  let disconnected = 0;
  class TestResizeObserver {
    observe(): void {
      observed += 1;
    }
    disconnect(): void {
      disconnected += 1;
    }
  }
  Object.assign(globalThis, { ResizeObserver: TestResizeObserver });
  try {
    const cleanup = enhanceMarquees(root);
    expect(root.querySelector("#summary .sb-marquee-content")).not.toBeNull();
    expect(root.querySelector("#body .sb-marquee-content")).toBeNull();
    expect(observed).toBe(2);

    if (details !== null) {
      details.open = true;
      details.dispatchEvent(new Event("toggle"));
    }
    expect(root.querySelector("#body .sb-marquee-content")).not.toBeNull();
    expect(observed).toBe(4);
    cleanup();
    expect(disconnected).toBe(2);
  } finally {
    Object.assign(globalThis, { ResizeObserver: original });
  }
});

test("carousel interval is optional, validated, and floor-clamped", () => {
  expect(carouselInterval(undefined)).toBeNull();
  expect(carouselInterval("")).toBeNull();
  expect(carouselInterval("oops")).toBeNull();
  expect(carouselInterval("-5")).toBeNull();
  expect(carouselInterval("200")).toBe(1_500);
  expect(carouselInterval("5000")).toBe(5_000);
});

test("carousel enhancer adds the slide class and cleans up its timers", () => {
  const root = container(
    '<div data-carousel data-carousel-interval="2000"><img alt="1"><img alt="2"></div>' +
      "<div data-carousel><span>manual</span></div>",
  );
  const cleanup = enhanceCarousels(root);
  const carousels = root.querySelectorAll(".sb-carousel");
  expect(carousels).toHaveLength(2);
  cleanup();
});

test("rotator interval defaults and floor-clamps; shifts map directions", () => {
  expect(rotatorInterval(undefined)).toBe(4_000);
  expect(rotatorInterval("")).toBe(4_000);
  expect(rotatorInterval("oops")).toBe(4_000);
  expect(rotatorInterval("100")).toBe(1_500);
  expect(rotatorInterval("6000")).toBe(6_000);
  // Content rolls away toward the direction, entering from the opposite.
  expect(rotatorShift(undefined)).toEqual({
    exit: ["0", "-100%"],
    enter: ["0", "100%"],
  });
  expect(rotatorShift("down").exit).toEqual(["0", "100%"]);
  expect(rotatorShift("left").enter).toEqual(["100%", "0"]);
  expect(rotatorShift("right").exit).toEqual(["100%", "0"]);
});

test.each([
  ["up", "translate(0, -100%)", "translate(0, 100%)"],
  ["down", "translate(0, 100%)", "translate(0, -100%)"],
  ["left", "translate(-100%, 0)", "translate(100%, 0)"],
  ["right", "translate(100%, 0)", "translate(-100%, 0)"],
])(
  "rotator rolls %s, pauses on hover and clears its movement",
  (direction, exit, enter) => {
    vi.useFakeTimers();
    try {
      const root = container(
        `<div data-rotator="${direction}" data-rotator-interval="2000">` +
          "<span>a</span><span>b</span><span>c</span></div>",
      );
      const element = root.querySelector<HTMLElement>("[data-rotator]");
      const [a, b] = [...root.querySelectorAll("span")];
      if (element === null || a === undefined || b === undefined)
        throw new Error("rotator fixture is incomplete");
      let entryTransform = "";
      vi.spyOn(b, "offsetWidth", "get").mockImplementation(() => {
        entryTransform = b.style.transform;
        return 0;
      });
      const cleanup = enhanceRotators(root);
      expect(a.classList.contains("sb-rotator-active")).toBe(true);

      element.dispatchEvent(new MouseEvent("mouseenter"));
      vi.advanceTimersByTime(2000);
      expect(a.classList.contains("sb-rotator-active")).toBe(true);
      expect(a.style.transform).toBe("");
      element.dispatchEvent(new MouseEvent("mouseleave"));
      vi.advanceTimersByTime(2000);
      expect(a.classList.contains("sb-rotator-leaving")).toBe(true);
      expect(a.style.transform).toBe(exit);
      expect(entryTransform).toBe(enter);
      expect(b.classList.contains("sb-rotator-active")).toBe(true);
      expect(b.style.transform).toBe("");

      // After settling, the leaver returns to the CSS baseline.
      vi.advanceTimersByTime(500);
      expect(a.classList.contains("sb-rotator-leaving")).toBe(false);
      expect(a.style.transform).toBe("");

      // Dispose during the following transition: neither timer may mutate it.
      vi.advanceTimersByTime(1500);
      expect(b.classList.contains("sb-rotator-leaving")).toBe(true);
      const beforeCleanup = root.innerHTML;
      cleanup();
      vi.advanceTimersByTime(10_000);
      expect(root.innerHTML).toBe(beforeCleanup);
    } finally {
      vi.restoreAllMocks();
      vi.useRealTimers();
    }
  },
);

test("reduced-motion rotators switch without a translated intermediate view", () => {
  vi.useFakeTimers();
  try {
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
    vi.spyOn(reduced, "matches", "get").mockReturnValue(true);
    vi.spyOn(window, "matchMedia").mockReturnValue(reduced);
    const root = container(
      '<div data-rotator="up"><span>a</span><span>b</span></div>',
    );
    const [a, b] = [...root.querySelectorAll("span")];
    if (a === undefined || b === undefined)
      throw new Error("rotator fixture is incomplete");
    const layout = vi.spyOn(b, "offsetWidth", "get");
    const cleanup = enhanceRotators(root);
    vi.advanceTimersByTime(4000);
    expect(a.classList.contains("sb-rotator-active")).toBe(false);
    expect(b.classList.contains("sb-rotator-active")).toBe(true);
    expect(a.style.transform).toBe("");
    expect(b.style.transform).toBe("");
    expect(layout).not.toHaveBeenCalled();
    cleanup();
  } finally {
    vi.restoreAllMocks();
    vi.useRealTimers();
  }
});

test.each([false, true])(
  "rotator preserves authored inline transforms with reduced motion %s",
  (reducedMotion) => {
    vi.useFakeTimers();
    try {
      const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
      vi.spyOn(reduced, "matches", "get").mockReturnValue(reducedMotion);
      vi.spyOn(window, "matchMedia").mockReturnValue(reduced);
      const root = container(
        '<div data-rotator="up" data-rotator-interval="1500">' +
          '<span style="transform: rotate(3deg) !important">a</span>' +
          '<span style="transform: none">b</span>' +
          '<span style="transform: scale(0.8) !important">c</span>' +
          "<span>d</span></div>",
      );
      const authoredViews = [...root.querySelectorAll<HTMLElement>("[style]")];
      const styles = () =>
        authoredViews.map((view) => view.getAttribute("style"));
      const before = styles();
      const incoming = authoredViews[1];
      if (incoming === undefined)
        throw new Error("rotator fixture is incomplete");
      vi.spyOn(incoming, "offsetWidth", "get").mockImplementation(() => {
        // Author styles also survive the temporary entering position.
        expect(styles()).toEqual(before);
        return 0;
      });
      const cleanup = enhanceRotators(root);
      expect(styles()).toEqual(before);
      for (let cycle = 0; cycle < 4; cycle += 1) {
        vi.advanceTimersByTime(1_500);
        // Includes the third view, untouched by this cycle's transition.
        expect(styles()).toEqual(before);
      }
      vi.advanceTimersByTime(400);
      expect(styles()).toEqual(before);
      cleanup();
      vi.advanceTimersByTime(10_000);
      expect(styles()).toEqual(before);
    } finally {
      vi.restoreAllMocks();
      vi.useRealTimers();
    }
  },
);

test("a single-view rotator stays put without timers", () => {
  const root = container("<div data-rotator><span>only</span></div>");
  const cleanup = enhanceRotators(root);
  expect(
    root.querySelector("span")?.classList.contains("sb-rotator-active"),
  ).toBe(true);
  cleanup();
});

test("tabs enhancer activates the first tab and switches on click", () => {
  const root = container(
    "<div data-tabs>" +
      '<div class="sb-tabs">' +
      '<button class="sb-tab" data-tab="a">A</button>' +
      '<button class="sb-tab" data-tab="b">B</button>' +
      "</div>" +
      '<div data-tab-panel="a">Panel A</div>' +
      '<div data-tab-panel="b">Panel B</div>' +
      "</div>",
  );
  document.body.appendChild(root);
  enhanceTabs(root);

  const tabs = root.querySelectorAll<HTMLElement>("[data-tab]");
  const panels = root.querySelectorAll<HTMLElement>("[data-tab-panel]");
  expect(tabs[0]?.classList.contains("sb-active")).toBe(true);
  expect(panels[0]?.hidden).toBe(false);
  expect(panels[1]?.hidden).toBe(true);

  tabs[1]?.click();
  expect(tabs[0]?.classList.contains("sb-active")).toBe(false);
  expect(tabs[1]?.classList.contains("sb-active")).toBe(true);
  expect(panels[0]?.hidden).toBe(true);
  expect(panels[1]?.hidden).toBe(false);
  root.remove();
});

test("tabs enhancer respects a pre-set sb-active tab", () => {
  const root = container(
    "<div data-tabs>" +
      '<button class="sb-tab" data-tab="a">A</button>' +
      '<button class="sb-tab sb-active" data-tab="b">B</button>' +
      '<div data-tab-panel="a">A</div>' +
      '<div data-tab-panel="b">B</div>' +
      "</div>",
  );
  enhanceTabs(root);
  const panels = root.querySelectorAll<HTMLElement>("[data-tab-panel]");
  expect(panels[0]?.hidden).toBe(true);
  expect(panels[1]?.hidden).toBe(false);
});

test("activateTab tolerates unknown ids by hiding every panel", () => {
  const root = container(
    '<div data-tabs><button data-tab="a">A</button><div data-tab-panel="a">A</div></div>',
  );
  const containerEl = root.querySelector<HTMLElement>("[data-tabs]");
  expect(containerEl).not.toBeNull();
  if (containerEl === null) return;
  activateTab(containerEl, "missing");
  expect(root.querySelector<HTMLElement>("[data-tab-panel]")?.hidden).toBe(
    true,
  );
});

test("rotator cleanup removes its hover listeners with the timers", () => {
  vi.useFakeTimers();
  try {
    const root = container(
      '<div data-rotator="up"><span>a</span><span>b</span></div>',
    );
    const element = root.querySelector<HTMLElement>("[data-rotator]");
    expect(element).not.toBeNull();
    if (element === null) return;
    const removed = vi.spyOn(element, "removeEventListener");
    const cleanup = enhanceRotators(root);
    cleanup();
    expect(removed.mock.calls.map(([type]) => type).sort()).toEqual([
      "mouseenter",
      "mouseleave",
    ]);
  } finally {
    vi.useRealTimers();
  }
});

test("tabs stop switching once their cleanup ran", () => {
  const root = container(
    '<div data-tabs><button data-tab="a" class="sb-active">A</button>' +
      '<button data-tab="b">B</button>' +
      '<div data-tab-panel="a">1</div><div data-tab-panel="b">2</div></div>',
  );
  const cleanup = enhanceTabs(root);
  const [a, b] = [...root.querySelectorAll<HTMLElement>("[data-tab]")];
  b?.click();
  expect(b?.classList.contains("sb-active")).toBe(true);
  cleanup();
  a?.click();
  expect(b?.classList.contains("sb-active")).toBe(true);
  expect(a?.classList.contains("sb-active")).toBe(false);
});
