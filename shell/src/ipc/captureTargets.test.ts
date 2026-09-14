// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";

import { collectTargets, stageCapture } from "./captureTargets";

function build(html: string): void {
  document.body.innerHTML = html;
}

/** The lint config forbids both `as HTMLElement` and `!`, and a test that
 *  silently gets `null` would fail somewhere unhelpful anyway. */
function element(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (el === null) throw new Error(`fixture is missing #${id}`);
  return el;
}

describe("collectTargets", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("names the dock, the tiles, the shortcuts and the open surfaces", () => {
    build(`
      <div data-bar-dock>
        <button data-shortcut-id="firefox"></button>
        <button data-tile-id="plugin:clock:clock"></button>
        <button data-tile-id="plugin:weather:current">
          <div data-tile-id="plugin:weather:current" data-plugin-id="weather"></div>
        </button>
      </div>
      <div data-capture="flyout"></div>
    `);
    expect([...collectTargets().keys()].sort()).toEqual([
      "bar",
      "flyout",
      "plugin:clock:clock",
      "plugin:weather:current",
      "shortcut:firefox",
    ]);
  });

  it("keeps the tile, not the shadow host, for a plugin tile", () => {
    build(`
      <button id="tile" data-tile-id="plugin:a:b">
        <div id="host" data-tile-id="plugin:a:b"></div>
      </button>
    `);
    expect(collectTargets().get("plugin:a:b")?.id).toBe("tile");
  });
});

describe("stageCapture", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("lifts the clipping of a scrollable subject and restores it after", () => {
    build(`<div id="list" style="max-height: 100px; overflow-y: auto"></div>`);
    const list = element("list");
    // happy-dom reports zero for both, so drive the scroll check explicitly.
    Object.defineProperty(list, "scrollHeight", {
      value: 900,
      configurable: true,
    });
    Object.defineProperty(list, "clientHeight", {
      value: 100,
      configurable: true,
    });

    const staged = stageCapture(list);
    expect(staged.expanded).toBe(true);
    expect(list.style.getPropertyValue("max-height")).toBe("none");
    expect(list.style.getPropertyValue("overflow-y")).toBe("visible");
    // A fixed height beats max-height, so it has to go as well.
    expect(list.style.getPropertyValue("height")).toBe("auto");

    // Freed, so no longer clipping — even though it still overflows.
    expect(staged.clipped).toBe(false);

    staged.release();
    expect(list.style.getPropertyValue("max-height")).toBe("100px");
    expect(list.style.getPropertyValue("overflow-y")).toBe("auto");
    expect(list.style.getPropertyValue("height")).toBe("");
  });

  it("keeps the height of a subject that only scrolls sideways", () => {
    // A CSS bar chart: fixed height, columns wider than the flyout.
    build(`<div id="chart" style="height: 120px; overflow-x: auto"></div>`);
    const chart = element("chart");
    Object.defineProperty(chart, "scrollWidth", {
      value: 400,
      configurable: true,
    });
    Object.defineProperty(chart, "clientWidth", {
      value: 300,
      configurable: true,
    });

    const staged = stageCapture(chart);
    expect(staged.expanded).toBe(true);
    expect(chart.style.getPropertyValue("overflow-x")).toBe("visible");
    expect(chart.style.getPropertyValue("max-width")).toBe("none");
    // The vertical budget is untouched: the bars keep the height they scale to.
    expect(chart.style.getPropertyValue("height")).toBe("120px");
    expect(chart.style.getPropertyValue("overflow-y")).toBe("visible");
    staged.release();
    expect(chart.style.getPropertyValue("overflow-x")).toBe("auto");
    expect(chart.style.getPropertyValue("overflow-y")).toBe("");
  });

  it("releases an inset-constrained scroll viewport and restores its anchors", () => {
    build(
      '<div id="settings" style="position: absolute; top: 0; bottom: 0; overflow-x: hidden; overflow-y: auto"></div>',
    );
    const settings = element("settings");
    Object.defineProperty(settings, "scrollHeight", {
      value: 1800,
      configurable: true,
    });
    Object.defineProperty(settings, "clientHeight", {
      value: 800,
      configurable: true,
    });
    const staged = stageCapture(settings);
    expect(staged.expanded).toBe(true);
    expect(staged.clipped).toBe(false);
    expect(settings.style.bottom).toBe("auto");
    expect(settings.style.overflowX).toBe("visible");
    expect(settings.style.overflowY).toBe("visible");
    staged.release();
    expect(settings.style.bottom).toBe("0px");
    expect(settings.style.top).toBe("0px");
    expect(settings.style.overflowX).toBe("hidden");
    expect(settings.style.overflowY).toBe("auto");
  });

  it("ignores content that overflows visibly instead of being cut", () => {
    build(`<div id="spill" style="overflow-y: visible"></div>`);
    const spill = element("spill");
    Object.defineProperty(spill, "scrollHeight", {
      value: 900,
      configurable: true,
    });
    Object.defineProperty(spill, "clientHeight", {
      value: 100,
      configurable: true,
    });

    const staged = stageCapture(spill);
    // Nothing is hidden, so there is nothing to free and nothing to warn about.
    expect(staged.expanded).toBe(false);
    expect(staged.clipped).toBe(false);
    expect(spill.getAttribute("style")).toBe("overflow-y: visible");
    staged.release();
  });

  it("frees a child before re-checking its parent", () => {
    build(`
      <div id="panel" style="overflow-y: auto">
        <div id="inner" style="height: 500px; overflow-y: auto"></div>
      </div>
    `);
    const panel = element("panel");
    const inner = element("inner");
    // The parent only reports as scrolling once the child has grown, which is
    // exactly what innermost-first ordering is for.
    Object.defineProperty(inner, "scrollHeight", {
      value: 900,
      configurable: true,
    });
    Object.defineProperty(inner, "clientHeight", {
      value: 500,
      configurable: true,
    });
    let parentGrew = false;
    Object.defineProperty(panel, "scrollHeight", {
      get: () => (parentGrew ? 900 : 500),
      configurable: true,
    });
    Object.defineProperty(panel, "clientHeight", {
      value: 500,
      configurable: true,
    });
    const originalSet = inner.style.setProperty.bind(inner.style);
    inner.style.setProperty = (
      prop: string,
      value: string | null,
      priority?: string,
    ): void => {
      if (prop === "height") parentGrew = true;
      originalSet(prop, value, priority);
    };

    const staged = stageCapture(panel);
    expect(panel.style.getPropertyValue("overflow-y")).toBe("visible");
    staged.release();
  });

  it("leaves a subject that does not scroll untouched", () => {
    build(`<div id="tile" style="width: 120px"></div>`);
    const tile = element("tile");
    const staged = stageCapture(tile);
    expect(staged.expanded).toBe(false);
    expect(staged.clipped).toBe(false);
    expect(tile.getAttribute("style")).toBe("width: 120px");
    staged.release();
    expect(tile.getAttribute("style")).toBe("width: 120px");
  });

  it("measures content that changes between staging and the painted reply", () => {
    build('<div id="subject"></div>');
    const subject = element("subject");
    let height = 100;
    subject.getBoundingClientRect = () =>
      DOMRect.fromRect({ width: 300, height });
    const staged = stageCapture(subject);
    height = 400;
    expect(staged.rect.h).toBe(400);
    staged.release();
  });

  it("anchors a subject that reaches past the viewport, then restores it", () => {
    build(
      `<div id="tall" style="position: fixed; left: 40px; top: 20px"></div>`,
    );
    const tall = element("tall");
    tall.getBoundingClientRect = () =>
      DOMRect.fromRect({
        x: 40,
        y: 20,
        width: 300,
        height: 4000,
      });

    const staged = stageCapture(tall);
    expect(tall.style.getPropertyValue("position")).toBe("absolute");
    expect(document.documentElement.style.getPropertyValue("overflow-y")).toBe(
      "visible",
    );
    expect(staged.rect.w).toBe(300);

    staged.release();
    expect(tall.style.getPropertyValue("position")).toBe("fixed");
    expect(document.documentElement.style.getPropertyValue("overflow-y")).toBe(
      "",
    );
  });
});
