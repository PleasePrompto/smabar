// @vitest-environment happy-dom
import { beforeEach, expect, test, vi } from "vitest";

import { lintMarkup, reportMarkupLint } from "./markupLint";
import { resetMarkupReports } from "./markupReport";
import { sanitizeHtml } from "./sanitize";

const logged: { level: string; message: string; options?: unknown }[] = [];

vi.mock("../ipc/log", () => ({
  uiLog: (level: string, message: string, options?: unknown) => {
    logged.push({ level, message, options });
  },
}));

function markupFor(html: string): DocumentFragment {
  return sanitizeHtml(html, () => null);
}

function findings(html: string, target = "flyout") {
  return lintMarkup(markupFor(html), target, "weather", "w").map(
    (finding) => `${finding.kind}:${finding.what}`,
  );
}

beforeEach(() => {
  logged.length = 0;
  resetMarkupReports();
});

test("spacing, type and flex set inline are flagged with the class to use", () => {
  expect(
    findings(
      '<div style="margin-top:4px; padding: 0 2px"></div><span style="font-size:11px"></span>' +
        '<p style="text-align:center; display: flex; gap: 4px"></p>',
    ),
  ).toEqual([
    "inlineStyle:div[style: margin-top]",
    "inlineStyle:div[style: padding]",
    "inlineStyle:span[style: font-size]",
    "inlineStyle:p[style: text-align]",
    "inlineStyle:p[style: display]",
    "inlineStyle:p[style: gap]",
  ]);
  reportMarkupLint(
    "weather",
    "w",
    "flyout",
    markupFor(
      '<div style="margin-top:4px"></div><b style="font-size:9px">x</b>',
    ),
  );
  expect(logged).toHaveLength(1);
  expect(logged[0]?.message).toContain('inline style in "w" (flyout)');
  expect(logged[0]?.message).toContain("sb-stack");
  expect(logged[0]?.message).toContain("sb-text-");
  expect(logged[0]?.message).not.toContain("sb-center");
});

test("data values, tokens and sizing caps stay allowed", () => {
  expect(
    findings(
      '<div class="sb-progress"><span style="width:38%"></span></div>' +
        '<div style="--sb-chart-value: 17; --sb-grid-cols: 3"></div>' +
        '<span style="max-width:9rem; max-height: 4rem; height: 2rem; background: var(--sb-accent)"></span>' +
        '<div style="display:block"></div>',
    ),
  ).toEqual([]);
});

test("a fixed width is a tile problem, not a flyout one", () => {
  const html =
    '<div class="sb-tile" style="width: 120px; min-width: 6rem">x</div>';
  expect(findings(html, "tile")).toEqual([
    "inlineStyle:div[style: width]",
    "inlineStyle:div[style: min-width]",
  ]);
  expect(findings(html, "flyout")).toEqual([]);
});

test("svg presentation attributes are not inline style problems", () => {
  expect(
    findings(
      '<svg viewBox="0 0 24 24"><rect style="margin:1px" x="1" y="1" width="2" height="2"/></svg>',
    ),
  ).toEqual([]);
});

test("an icon-only control needs a name", () => {
  expect(
    findings(
      '<button data-action="refresh"><span data-lucide="refresh-cw"></span></button>' +
        '<button data-action="ok" title="Ok"><span data-lucide="check"></span></button>' +
        '<button aria-label="Close"><svg></svg></button>' +
        '<button data-action="save">Save</button>' +
        '<a data-action="open"><img alt="Open"></a>' +
        '<span data-action="x"><span data-lucide="x"></span></span>',
    ),
  ).toEqual([
    'unnamedControl:button[data-action="refresh"]',
    'unnamedControl:span[data-action="x"]',
  ]);
  reportMarkupLint(
    "weather",
    "w",
    "flyout",
    markupFor(
      '<button data-action="refresh"><span data-lucide="refresh-cw"></span></button>',
    ),
  );
  expect(logged[0]?.message).toContain("unnamed icon-only controls");
  expect(logged[0]?.message).toContain("title=");
  expect(logged[0]?.message).toContain("aria-label=");
});

test("raw locale keys are told apart from versions, domains and literals", () => {
  expect(
    findings(
      "<p>weather.title</p><p>w.state</p><p>a.b.c</p>" +
        "<p>smabar.com</p><p>node.js</p><p>v1.2.3</p><p>1.2.3</p><p>e.g.</p>" +
        '<code>x.y.z</code><a href="https://x.y">x.y.z</a>',
    ),
  ).toEqual([
    "localeKey:weather.title",
    "localeKey:w.state",
    "localeKey:a.b.c",
  ]);
  reportMarkupLint("weather", "w", "tile", markupFor("<b>weather.title</b>"));
  expect(logged[0]?.message).toContain("raw locale keys");
  expect(logged[0]?.message).toContain("on_ready");
});

test("a KPI value that cannot fit its cell is flagged", () => {
  expect(
    findings(
      '<div class="sb-kpi"><span class="sb-kpi-value">1,234,567.89 GB</span></div>' +
        '<div class="sb-kpi"><span class="sb-kpi-value">38 %</span></div>',
    ),
  ).toEqual(['kpiValue:"1,234,567.89 GB" (15 chars)']);
});

test("each kind is reported once per surface and re-arms after a clean render", () => {
  const bad = () =>
    markupFor(
      '<div style="margin:0"></div><button data-action="r"><svg></svg></button>',
    );
  reportMarkupLint("weather", "w", "flyout", bad());
  reportMarkupLint("weather", "w", "flyout", bad());
  expect(logged.map((entry) => entry.message.split(" in ")[0])).toEqual([
    "inline style",
    "unnamed icon-only controls",
  ]);
  reportMarkupLint("weather", "w", "flyout", markupFor("<p>clean</p>"));
  reportMarkupLint("weather", "w", "flyout", bad());
  expect(logged).toHaveLength(4);
  expect(logged[0]?.options).toMatchObject({
    pluginId: "weather",
    fields: { tileId: "w", target: "flyout", kind: "inlineStyle" },
  });
});
