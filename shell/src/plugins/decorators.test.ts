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

test("rotator shows the first view and rolls to the next on schedule", () => {
  vi.useFakeTimers();
  try {
    const root = container(
      '<div data-rotator="up" data-rotator-interval="2000">' +
        "<span>a</span><span>b</span><span>c</span></div>",
    );
    const cleanup = enhanceRotators(root);
    const [a, b] = [...root.querySelectorAll("span")];
    expect(root.querySelector(".sb-rotator")).not.toBeNull();
    expect(a?.classList.contains("sb-rotator-active")).toBe(true);

    vi.advanceTimersByTime(2_050);
    expect(a?.classList.contains("sb-rotator-leaving")).toBe(true);
    expect(b?.classList.contains("sb-rotator-active")).toBe(true);

    // After the settle timeout the leaver returns to the parked base state.
    vi.advanceTimersByTime(500);
    expect(a?.classList.contains("sb-rotator-leaving")).toBe(false);

    cleanup();
    const active = root.querySelectorAll(".sb-rotator-active").length;
    vi.advanceTimersByTime(10_000);
    // Cleanup stopped the timer — nothing advances any more.
    expect(root.querySelectorAll(".sb-rotator-active")).toHaveLength(active);
  } finally {
    vi.useRealTimers();
  }
});

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
