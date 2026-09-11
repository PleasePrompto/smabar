// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";

import { captureChartMemory, clearChartMemory, enhanceCharts } from "./charts";
import { enhanceIcons, ICONS, renderIcon } from "./icons";
import { container } from "./pluginTestDom";

test("enhanceIcons replaces data-lucide content with a trusted svg", () => {
  const root = container('<span data-lucide="cpu">fallback text</span>');
  enhanceIcons(root);
  const svg = root.querySelector("span[data-lucide] > svg");
  expect(svg).not.toBeNull();
  expect(svg?.getAttribute("viewBox")).toBe("0 0 24 24");
  expect(svg?.getAttribute("stroke")).toBe("currentColor");
  expect(svg?.childElementCount).toBe(ICONS.cpu?.length);
  // The placeholder's own content is gone.
  expect(root.textContent).toBe("");
});

test("unknown icon names fall back to the circle glyph", () => {
  const root = container('<span data-lucide="no-such-icon"></span>');
  enhanceIcons(root);
  const svg = root.querySelector("svg");
  expect(svg).not.toBeNull();
  expect(svg?.childElementCount).toBe(ICONS.circle?.length);
});

test("renderIcon never emits the react key bookkeeping attribute", () => {
  const svg = renderIcon("cpu", document);
  for (const shape of svg.children) {
    expect(shape.hasAttribute("key")).toBe(false);
  }
});

/** The fill circle: dasharray "100 100", the shown percent is 100 − offset. */
function donutOffset(root: ParentNode): string {
  const fill = root.querySelector<SVGCircleElement>("circle.sb-donut-fill");
  if (fill === null) throw new Error("no donut fill circle");
  return fill.style.strokeDashoffset;
}

test("donut renders a two-circle ring and keeps the center label", () => {
  const root = container(
    '<div data-chart="donut" data-value="62"><span>62%</span></div>',
  );
  enhanceCharts(root);
  const circles = root.querySelectorAll("svg circle");
  expect(circles).toHaveLength(2);
  expect(circles[1]?.getAttribute("stroke")).toBe("currentColor");
  // Dashoffset technique: the circumference is 100 units, the offset hides
  // the remainder — 62% shown leaves an offset of 38.
  expect(circles[1]?.getAttribute("stroke-dasharray")).toBe("100 100");
  expect(donutOffset(root)).toBe("38");
  expect(root.textContent).toBe("62%");
});

test("donut clamps out-of-range values and honors data-max", () => {
  const root = container(
    '<div data-chart="donut" data-value="4" data-max="8"></div>',
  );
  enhanceCharts(root);
  expect(donutOffset(root)).toBe("50");

  const over = container('<div data-chart="donut" data-value="200"></div>');
  enhanceCharts(over);
  expect(donutOffset(over)).toBe("0");
});

test("a keyed donut starts at its previous value and glides to the new one", async () => {
  // Same memory key = same tile surface across two renders.
  const first = container('<div data-chart="donut" data-value="20"></div>');
  enhanceCharts(first, "plug/w/tile");
  expect(donutOffset(first)).toBe("80");

  const second = container('<div data-chart="donut" data-value="60"></div>');
  enhanceCharts(second, "plug/w/tile");
  // The rebuilt ring is painted at the OLD value first…
  expect(donutOffset(second)).toBe("80");
  // …and reaches the new one after the double-rAF flush.
  await new Promise((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(resolve)),
  );
  expect(donutOffset(second)).toBe("40");

  // A different key is a different tile: no inherited start value.
  const other = container('<div data-chart="donut" data-value="60"></div>');
  enhanceCharts(other, "plug/other/tile");
  expect(donutOffset(other)).toBe("40");
});

test("a keyed progress bar glides from its previous width", async () => {
  const first = container(
    '<div class="sb-progress"><span style="width: 30%"></span></div>',
  );
  enhanceCharts(first, "plug/p/tile");
  expect(first.querySelector("span")?.style.width).toBe("30%");

  const second = container(
    '<div class="sb-progress"><span style="width: 70%"></span></div>',
  );
  enhanceCharts(second, "plug/p/tile");
  expect(second.querySelector("span")?.style.width).toBe("30%");
  await new Promise((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(resolve)),
  );
  expect(second.querySelector("span")?.style.width).toBe("70%");
});

test("stable chart keys survive reordering without borrowing values", () => {
  const first = container(
    '<div data-chart="donut" data-sb-key="cpu" data-value="20"></div>' +
      '<div data-chart="donut" data-sb-key="ram" data-value="40"></div>',
  );
  enhanceCharts(first, "plug/keyed/tile");

  const reordered = container(
    '<div data-chart="donut" data-sb-key="ram" data-value="60"></div>' +
      '<div data-chart="donut" data-sb-key="cpu" data-value="80"></div>',
  );
  enhanceCharts(reordered, "plug/keyed/tile");
  const offsets = [...reordered.querySelectorAll("circle.sb-donut-fill")].map(
    (fill) => (fill as SVGCircleElement).style.strokeDashoffset,
  );
  expect(offsets).toEqual(["60", "80"]);
});

test("multiple unkeyed charts never inherit index-based memory", () => {
  enhanceCharts(
    container(
      '<div data-chart="donut" data-value="20"></div>' +
        '<div data-chart="donut" data-value="40"></div>',
    ),
    "plug/unkeyed/tile",
  );
  const reordered = container(
    '<div data-chart="donut" data-value="60"></div>' +
      '<div data-chart="donut" data-value="80"></div>',
  );
  enhanceCharts(reordered, "plug/unkeyed/tile");
  const offsets = [...reordered.querySelectorAll("circle.sb-donut-fill")].map(
    (fill) => (fill as SVGCircleElement).style.strokeDashoffset,
  );
  expect(offsets).toEqual(["40", "20"]);
});

test('the public chart key "only" cannot inherit the single-chart fallback', () => {
  enhanceCharts(
    container('<div data-chart="donut" data-value="20"></div>'),
    "plug/only/tile",
  );
  const keyed = container(
    '<div data-chart="donut" data-sb-key="only" data-value="80"></div>' +
      '<div data-chart="donut" data-sb-key="other" data-value="60"></div>',
  );
  enhanceCharts(keyed, "plug/only/tile");
  const offsets = [...keyed.querySelectorAll("circle.sb-donut-fill")].map(
    (fill) => (fill as SVGCircleElement).style.strokeDashoffset,
  );
  expect(offsets).toEqual(["20", "40"]);
});

test("an interrupted chart glide commits only the newest target", async () => {
  enhanceCharts(
    container(
      '<div data-chart="donut" data-sb-key="cpu" data-value="20"></div>',
    ),
    "plug/interrupted/tile",
  );
  enhanceCharts(
    container(
      '<div data-chart="donut" data-sb-key="cpu" data-value="60"></div>',
    ),
    "plug/interrupted/tile",
  );
  const newest = container(
    '<div data-chart="donut" data-sb-key="cpu" data-value="80"></div>',
  );
  enhanceCharts(newest, "plug/interrupted/tile");
  await new Promise((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(resolve)),
  );
  expect(donutOffset(newest)).toBe("20");

  const following = container(
    '<div data-chart="donut" data-sb-key="cpu" data-value="100"></div>',
  );
  enhanceCharts(following, "plug/interrupted/tile");
  expect(donutOffset(following)).toBe("20");
});

test("a render during a CSS transition resumes from the painted value", () => {
  const scope = "plug/painted/tile";
  const previous = container(
    '<div data-chart="donut" data-value="20"></div>' +
      '<div class="sb-progress"><span style="width: 20%"></span></div>',
  );
  enhanceCharts(previous, scope);
  const fill = previous.querySelector<SVGCircleElement>("circle.sb-donut-fill");
  const bar = previous.querySelector<HTMLElement>(".sb-progress > span");
  if (fill === null || bar === null) throw new Error("charts did not render");
  fill.style.strokeDashoffset = "55";
  bar.style.width = "45%";
  captureChartMemory(previous, scope);

  const next = container(
    '<div data-chart="donut" data-value="80"></div>' +
      '<div class="sb-progress"><span style="width: 90%"></span></div>',
  );
  enhanceCharts(next, scope);
  expect(donutOffset(next)).toBe("55");
  expect(
    next.querySelector<HTMLElement>(".sb-progress > span")?.style.width,
  ).toBe("45%");
});

test("a stale chart frame cannot reuse a remounted scope revision", () => {
  const frames: FrameRequestCallback[] = [];
  vi.stubGlobal(
    "requestAnimationFrame",
    (callback: FrameRequestCallback): number => frames.push(callback),
  );
  try {
    const scope = "plug/remounted/tile";
    enhanceCharts(
      container('<div data-chart="donut" data-value="20"></div>'),
      scope,
    );
    enhanceCharts(
      container('<div data-chart="donut" data-value="60"></div>'),
      scope,
    );
    frames.shift()?.(0); // old glide's inner frame is now pending

    clearChartMemory(scope);
    enhanceCharts(
      container('<div data-chart="donut" data-value="80"></div>'),
      scope,
    );
    enhanceCharts(
      container('<div data-chart="donut" data-value="100"></div>'),
      scope,
    );
    const pending = frames.splice(0);
    for (const frame of pending) frame(16);

    const following = container(
      '<div data-chart="donut" data-value="40"></div>',
    );
    enhanceCharts(following, scope);
    expect(donutOffset(following)).toBe("20");
  } finally {
    vi.unstubAllGlobals();
  }
});

test("broken donut numbers render nothing", () => {
  for (const html of [
    '<div data-chart="donut"></div>',
    '<div data-chart="donut" data-value="oops"></div>',
    '<div data-chart="donut" data-value="5" data-max="0"></div>',
  ]) {
    const root = container(html);
    enhanceCharts(root);
    expect(root.querySelector("svg"), html).toBeNull();
  }
});

test("sparkline renders a normalized polyline", () => {
  const root = container(
    '<div data-chart="sparkline" data-points="1, 3 ,2"></div>',
  );
  enhanceCharts(root);
  const line = root.querySelector("svg polyline");
  expect(line).not.toBeNull();
  expect(line?.getAttribute("stroke")).toBe("currentColor");
  const points = line?.getAttribute("points")?.split(" ");
  expect(points).toHaveLength(3);
});

test("broken sparkline points render nothing", () => {
  for (const html of [
    '<div data-chart="sparkline"></div>',
    '<div data-chart="sparkline" data-points="7"></div>',
    '<div data-chart="sparkline" data-points="1,oops,3"></div>',
  ]) {
    const root = container(html);
    enhanceCharts(root);
    expect(root.querySelector("svg"), html).toBeNull();
  }
});
