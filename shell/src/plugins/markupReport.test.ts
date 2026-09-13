// @vitest-environment happy-dom
import { beforeEach, expect, test, vi } from "vitest";

import {
  MAX_PROBLEMS_PER_SLOT,
  rememberProblems,
  reportMarkupDrops,
  reportUnknownKitClasses,
  resetMarkupReports,
} from "./markupReport";
import { sanitizeHtml, type SanitizerDrop } from "./sanitize";

const logged: { level: string; message: string; options?: unknown }[] = [];

vi.mock("../ipc/log", () => ({
  uiLog: (level: string, message: string, options?: unknown) => {
    logged.push({ level, message, options });
  },
}));

/** Runs the sanitizer the way the render path does and returns the drops. */
function dropsFor(html: string): SanitizerDrop[] {
  const drops: SanitizerDrop[] = [];
  sanitizeHtml(
    html,
    () => null,
    (drop) => drops.push(drop),
  );
  return drops;
}

function markupFor(html: string): DocumentFragment {
  return sanitizeHtml(html, () => null);
}

beforeEach(() => {
  logged.length = 0;
  resetMarkupReports();
});

test("clean markup reports nothing", () => {
  expect(dropsFor('<div class="sb-card"><p>hi</p></div>')).toEqual([]);
  reportMarkupDrops("clock", "w", "flyout", []);
  expect(logged).toHaveLength(0);
});

test("a form that tries to navigate is corrected, not silently broken", () => {
  const drops = dropsFor('<form action="https://x" method="post"></form>');
  expect(drops.map((drop) => drop.what)).toEqual([
    "form[action]",
    "form[method]",
  ]);
  expect(drops[0]?.reason).toContain("data-action");
});

test("every silent removal is reported with a reason", () => {
  const cases: [string, string][] = [
    ["<script>alert(1)</script>", "script"],
    ["<style>p{}</style>", "style"],
    ["<iframe></iframe>", "iframe[title]"],
    ["<template><b>x</b></template>", "template"],
    ['<div onclick="x()"></div>', "div[onclick]"],
    ['<img src="file:///etc/passwd">', "img[src]"],
    ['<img src="sb-asset:../escape.png">', "img[src]"],
    ['<a href="ftp://x/y">l</a>', "a[href]"],
    ['<input type="file">', "input[type]"],
    ["<marquee>x</marquee>", "marquee"],
    ['<svg><use href="x#i"></use></svg>', "use"],
    ['<svg><path fill="javascript:alert(1)"></path></svg>', "path[fill]"],
    ["<svg><foreignObject></foreignObject></svg>", "foreignObject"],
    ['<button command="--custom" commandfor="d">x</button>', "button[command]"],
  ];
  for (const [html, what] of cases) {
    const drops = dropsFor(html);
    const hit = drops.find((drop) => drop.what === what);
    expect(hit, `${html} must report ${what}`).toBeDefined();
    expect(hit?.reason.length ?? 0).toBeGreaterThan(20);
  }
});

test("a javascript: URL is reported wherever it hides", () => {
  const drops = dropsFor('<a href="java\tscript:alert(1)">x</a>');
  expect(drops[0]?.reason).toContain("data-action");
});

test("the same problem is reported once, not once per render", () => {
  const drops = dropsFor('<script></script><div onclick="x"></div>');
  for (let i = 0; i < 20; i += 1) {
    reportMarkupDrops("clock", "w", "flyout", drops);
  }
  expect(logged).toHaveLength(1);
  expect(logged[0]?.message).toContain("markup removed");
  expect(logged[0]?.message).toContain("script");
  expect(logged[0]?.message).toContain("div[onclick]");
});

test("counts are summarised and repeats collapse", () => {
  const drops = dropsFor('<div onclick="a"></div><div onclick="b"></div>');
  reportMarkupDrops("clock", "w", "tile", drops);
  expect(logged[0]?.message).toContain("div[onclick] (2)");
});

test("each new problem is reported once across overlapping renders", () => {
  reportMarkupDrops("clock", "w", "flyout", dropsFor("<style></style>"));
  reportMarkupDrops(
    "clock",
    "w",
    "flyout",
    dropsFor("<style></style><script></script>"),
  );
  reportMarkupDrops("clock", "w", "flyout", dropsFor("<style></style>"));
  expect(logged).toHaveLength(2);
  expect(logged[1]?.message).toContain("script");
  expect(logged[1]?.message).not.toContain("style");
});

test("a clean render re-arms the same problem", () => {
  // Fixed and reintroduced is a new event worth a new line; the contract
  // promises exactly this re-arm.
  const bad = dropsFor("<script></script>");
  reportMarkupDrops("clock", "w", "flyout", bad);
  reportMarkupDrops("clock", "w", "flyout", bad);
  reportMarkupDrops("clock", "w", "flyout", []);
  reportMarkupDrops("clock", "w", "flyout", bad);
  expect(logged).toHaveLength(2);
});

test("unknown classes re-arm after a clean render", () => {
  const bad = () => markupFor('<div class="sb-missing"></div>');
  reportUnknownKitClasses("clock", "w", "flyout", bad());
  reportUnknownKitClasses("clock", "w", "flyout", bad());
  reportUnknownKitClasses(
    "clock",
    "w",
    "flyout",
    markupFor('<div class="sb-card"></div>'),
  );
  reportUnknownKitClasses("clock", "w", "flyout", bad());
  expect(logged).toHaveLength(2);
});

test("the same removal target with a different cause is a new problem", () => {
  const fileUrl = dropsFor('<img src="file:///etc/passwd">');
  const escapedAsset = dropsFor('<img src="sb-asset:../escape.png">');
  expect(fileUrl[0]?.what).toBe("img[src]");
  expect(escapedAsset[0]?.what).toBe("img[src]");
  expect(fileUrl[0]?.reason).not.toBe(escapedAsset[0]?.reason);

  reportMarkupDrops("clock", "w", "flyout", fileUrl);
  reportMarkupDrops("clock", "w", "flyout", escapedAsset);
  reportMarkupDrops("clock", "w", "flyout", fileUrl);

  expect(logged).toHaveLength(2);
  expect(logged[1]?.message).toContain("img[src]");
});

test("the entry is routed into the plugin's own log", () => {
  reportMarkupDrops("clock", "world", "flyout", dropsFor("<script></script>"));
  expect(logged[0]?.level).toBe("warn");
  expect(logged[0]?.options).toMatchObject({
    pluginId: "clock",
    fields: { tileId: "world", target: "flyout" },
  });
});

test("tiles are tracked separately", () => {
  const drops = dropsFor("<script></script>");
  reportMarkupDrops("clock", "a", "tile", drops);
  reportMarkupDrops("clock", "b", "tile", drops);
  expect(logged).toHaveLength(2);
});

test("classes provided by either loaded kit stylesheet are accepted", () => {
  const markup = markupFor(
    '<div class="plain sb-card sb-active sb-btn-icon sb-accordion"></div>',
  );
  reportUnknownKitClasses("clock", "w", "flyout", markup);
  expect(logged).toHaveLength(0);
});

test("unknown sb classes are preserved and reported once in stable order", () => {
  const html =
    '<div class="plain sb-future sb-cardd"><span class="sb-cardd"></span></div>';
  const markup = markupFor(html);
  reportUnknownKitClasses("clock", "w", "tile", markup);

  expect(markup.querySelector("div")?.className).toBe(
    "plain sb-future sb-cardd",
  );
  expect(logged).toHaveLength(1);
  expect(logged[0]?.message).toContain(
    'unknown sb-* classes in "w" (tile): sb-cardd, sb-future',
  );
  expect(logged[0]?.message).toContain("use ui_kit");
  expect(logged[0]?.options).toMatchObject({
    pluginId: "clock",
    fields: {
      tileId: "w",
      target: "tile",
      unknownClasses: ["sb-cardd", "sb-future"],
    },
  });

  for (let i = 0; i < 20; i += 1) {
    reportUnknownKitClasses("clock", "w", "tile", markupFor(html));
  }
  expect(logged).toHaveLength(1);
});

test("unknown classes are each reported once across overlapping state changes", () => {
  const first = () => markupFor('<div class="sb-missing"></div>');
  const second = () => markupFor('<div class="sb-other"></div>');
  reportUnknownKitClasses("clock", "w", "flyout", first());
  reportUnknownKitClasses(
    "clock",
    "w",
    "flyout",
    markupFor('<div class="sb-missing sb-other"></div>'),
  );
  reportUnknownKitClasses("clock", "w", "flyout", second());
  reportUnknownKitClasses("clock", "w", "flyout", first());
  expect(logged).toHaveLength(2);
  expect(logged[1]?.message).toContain("sb-other");
  expect(logged[1]?.message).not.toContain("sb-missing");
});

test("unknown classes are deduplicated per tile and target", () => {
  const bad = () => markupFor('<div class="sb-missing"></div>');
  reportUnknownKitClasses("clock", "a", "tile", bad());
  reportUnknownKitClasses("clock", "a", "flyout", bad());
  reportUnknownKitClasses("clock", "b", "tile", bad());
  expect(logged).toHaveLength(3);
});

test("a value that changes every render cannot grow the memory without limit", () => {
  const fresh: string[] = [];
  for (let i = 0; i < MAX_PROBLEMS_PER_SLOT + 8; i += 1) {
    fresh.push(
      ...rememberProblems("kpiValue", "p/t/tile", [
        `"v${String(i)}" (13 chars)`,
      ]),
    );
  }
  expect(fresh).toHaveLength(MAX_PROBLEMS_PER_SLOT);
  // A clean render re-arms the slot as before.
  expect(rememberProblems("kpiValue", "p/t/tile", [])).toEqual([]);
  expect(
    rememberProblems("kpiValue", "p/t/tile", ['"again" (13 chars)']),
  ).toEqual(['"again" (13 chars)']);
});
