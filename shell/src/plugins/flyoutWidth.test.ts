// @vitest-environment happy-dom
import { expect, test } from "vitest";

import contract from "../../../ui-kit/contract.json";
import { sanitizeHtml, type SanitizerDrop } from "./sanitize";
import { DEFAULT_FLYOUT_WIDTH, readFlyoutWidth } from "./flyoutWidth";

test.each([
  ["wide", "42.5rem"],
  ["720", "720px"],
  [" 720 ", "720px"],
  ["1", "1px"],
  ["16384", "16384px"],
])("accepts the documented width %s", (value, expected) => {
  const drops: SanitizerDrop[] = [];
  const markup = sanitizeHtml(
    `\n<section data-sb-flyout-width="${value}">Content</section>\n`,
  );
  expect(readFlyoutWidth(markup, "flyout", (drop) => drops.push(drop))).toBe(
    expected,
  );
  expect(drops).toEqual([]);
});

test.each([
  "",
  "0",
  "-1",
  "1.5",
  "16385",
  "720px",
  "100%",
  "1e3",
  "NaN",
  "calc(1px)",
])("rejects invalid width %s with an actionable diagnostic", (value) => {
  const drops: SanitizerDrop[] = [];
  const markup = sanitizeHtml(
    `<section data-sb-flyout-width="${value}">Content</section>`,
  );
  expect(readFlyoutWidth(markup, "flyout", (drop) => drops.push(drop))).toBe(
    DEFAULT_FLYOUT_WIDTH,
  );
  expect(markup.querySelector("[data-sb-flyout-width]")).toBeNull();
  expect(drops).toHaveLength(1);
  expect(drops[0]?.reason).toContain("1 to 16384");
});

test.each([
  '<section><div data-sb-flyout-width="wide">Nested</div></section>',
  '<section data-sb-flyout-width="wide"></section><p>Sibling</p>',
  'Sibling text<section data-sb-flyout-width="wide"></section>',
  '<section data-sb-flyout-width="wide"><div data-sb-flyout-width="720"></div></section>',
])("rejects ambiguous or nested requests: %s", (html) => {
  const drops: SanitizerDrop[] = [];
  const markup = sanitizeHtml(html);
  expect(readFlyoutWidth(markup, "flyout", (drop) => drops.push(drop))).toBe(
    DEFAULT_FLYOUT_WIDTH,
  );
  expect(drops.length).toBeGreaterThan(0);
  expect(markup.querySelector("[data-sb-flyout-width]")).toBeNull();
});

test.each(["tile", "popup"])("width requests do not affect %s", (target) => {
  const drops: SanitizerDrop[] = [];
  expect(
    readFlyoutWidth(
      sanitizeHtml('<section data-sb-flyout-width="wide"></section>'),
      target,
      (drop) => drops.push(drop),
    ),
  ).toBe(DEFAULT_FLYOUT_WIDTH);
  expect(drops).toHaveLength(1);
});

test("missing or sanitized-away width uses the default without extra diagnostics", () => {
  const drops: SanitizerDrop[] = [];
  for (const html of [
    "",
    "Text",
    '<script data-sb-flyout-width="wide"></script>',
  ]) {
    expect(
      readFlyoutWidth(sanitizeHtml(html), "flyout", (drop) => drops.push(drop)),
    ).toBe(DEFAULT_FLYOUT_WIDTH);
  }
  expect(drops).toEqual([]);
});

test("the documented split example survives sanitizing and requests a wide flyout", () => {
  const drops: SanitizerDrop[] = [];
  const markup = sanitizeHtml(
    contract.snippets.splitFlyout,
    undefined,
    (drop) => drops.push(drop),
  );
  expect(readFlyoutWidth(markup, "flyout", (drop) => drops.push(drop))).toBe(
    "42.5rem",
  );
  expect(markup.querySelectorAll(".sb-split > *")).toHaveLength(2);
  expect(
    markup.querySelector('.sb-split > .sb-scroll[tabindex="0"][aria-label]'),
  ).not.toBeNull();
  expect(drops).toEqual([]);
});

test.each([
  [contract.snippets.wideFlyout, "42.5rem"],
  [contract.snippets.screenFlyout, "16384px"],
])(
  "single-scroll examples request their width without inner scroll panes",
  (html, width) => {
    const drops: SanitizerDrop[] = [];
    const markup = sanitizeHtml(html, undefined, (drop) => drops.push(drop));
    expect(readFlyoutWidth(markup, "flyout", (drop) => drops.push(drop))).toBe(
      width,
    );
    expect(markup.querySelector(".sb-split, .sb-scroll")).toBeNull();
    expect(drops).toEqual([]);
  },
);
