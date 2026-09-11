// @vitest-environment happy-dom
import { expect, test } from "vitest";

import defaultTheme from "../../../../themes/default.json";
import { readThemeDocument } from "../../theme/document";
import {
  BAR_BORDER_DEFAULT,
  BAR_CONTENT_FLOOR_PX,
  BAR_OPACITY_DEFAULT,
  BAR_PAD_Y_DEFAULT,
  BAR_RADIUS_DEFAULT,
  FLYOUT_OPACITY_DEFAULT,
  GAP_DEFAULT,
  ICON_SIZE_DEFAULT,
  LABEL_SIZE_DEFAULT,
  MAGNIFY_DEFAULT,
  MARGIN_DEFAULT,
  NEIGHBORS_DEFAULT,
  accentPrimaryTokens,
  accentSecondaryTokens,
  accentTokens,
  clampIconSize,
  clampLabelSize,
  clampMagnifyScale,
  clampMargin,
  clampNeighbors,
  moveItem,
  pxToRem,
  reorderWithHidden,
  surfaceTokens,
  textTokens,
  toggleDisabled,
  tokenNumber,
  whitespaceTokens,
} from "./model";

test("moveItem moves an element down and up", () => {
  expect(moveItem(["a", "b", "c"], 0, 1)).toEqual(["b", "a", "c"]);
  expect(moveItem(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
});

test("moveItem returns an unchanged copy for edge no-ops", () => {
  const items = ["a", "b", "c"];
  expect(moveItem(items, 0, -1)).toEqual(items);
  expect(moveItem(items, 2, 3)).toEqual(items);
  expect(moveItem(items, -1, 0)).toEqual(items);
  expect(moveItem(items, 1, 1)).toEqual(items);
});

test("moveItem never mutates its input", () => {
  const items = ["a", "b", "c"];
  moveItem(items, 0, 2);
  expect(items).toEqual(["a", "b", "c"]);
});

test("toggleDisabled adds an absent id and removes a present one", () => {
  expect(toggleDisabled([], "clock")).toEqual(["clock"]);
  expect(toggleDisabled(["clock", "x"], "clock")).toEqual(["x"]);
});

test("clampMagnifyScale clamps into 1.0–1.6", () => {
  expect(clampMagnifyScale(0.5)).toBe(1);
  expect(clampMagnifyScale(1.25)).toBe(1.25);
  expect(clampMagnifyScale(2)).toBe(1.6);
});

test("clampMagnifyScale falls back to the default for non-finite input", () => {
  expect(clampMagnifyScale(Number.NaN)).toBe(MAGNIFY_DEFAULT);
  expect(clampMagnifyScale(Number.POSITIVE_INFINITY)).toBe(MAGNIFY_DEFAULT);
});

test("size and margin clamps round into their integer ranges", () => {
  expect(clampIconSize(8)).toBe(16);
  expect(clampIconSize(48.4)).toBe(48);
  expect(clampIconSize(999)).toBe(64);

  expect(clampLabelSize(4)).toBe(9);
  expect(clampLabelSize(12)).toBe(12);
  expect(clampLabelSize(40)).toBe(16);

  expect(clampMargin(-5)).toBe(0);
  expect(clampMargin(12)).toBe(12);
  expect(clampMargin(200)).toBe(64);

  expect(clampNeighbors(-1)).toBe(0);
  expect(clampNeighbors(1.6)).toBe(2);
  expect(clampNeighbors(99)).toBe(3);
});

test("size and margin clamps fall back to the defaults for non-finite input", () => {
  expect(clampIconSize(Number.NaN)).toBe(ICON_SIZE_DEFAULT);
  expect(clampLabelSize(Number.NaN)).toBe(LABEL_SIZE_DEFAULT);
  expect(clampMargin(Number.NaN)).toBe(MARGIN_DEFAULT);
  expect(clampNeighbors(Number.POSITIVE_INFINITY)).toBe(NEIGHBORS_DEFAULT);
});

test("tokenNumber parses override values and falls back per token", () => {
  const tokens = { "--sb-tile-gap": "12px", "--sb-bar-opacity": "62%" };
  expect(tokenNumber(tokens, "--sb-tile-gap", 6)).toBe(12);
  expect(tokenNumber(tokens, "--sb-bar-opacity", 100)).toBe(62);
  expect(tokenNumber(tokens, "--sb-bar-radius", 24)).toBe(24);
  expect(
    tokenNumber({ "--sb-tile-gap": "calc(oops)" }, "--sb-tile-gap", 6),
  ).toBe(6);
  // rem tokens are converted: the sliders are px, themes write "1rem".
  expect(
    tokenNumber({ "--sb-bar-radius": "1rem" }, "--sb-bar-radius", 16),
  ).toBe(16);
  expect(
    tokenNumber({ "--sb-bar-radius": "0.625rem" }, "--sb-bar-radius", 24),
  ).toBe(10);
});

test("tokenNumber reads the active theme off :root when there is no override", () => {
  document.documentElement.style.setProperty("--sb-bar-opacity", "66%");
  document.documentElement.style.setProperty("--sb-bar-radius", "0.625rem");
  try {
    expect(tokenNumber({}, "--sb-bar-opacity", 55)).toBe(66);
    expect(tokenNumber({}, "--sb-bar-radius", 24)).toBe(10);
    // An override still wins over the theme value.
    expect(
      tokenNumber({ "--sb-bar-opacity": "20%" }, "--sb-bar-opacity", 55),
    ).toBe(20);
    // Unknown tokens fall through to the constant.
    expect(tokenNumber({}, "--sb-tile-gap", 6)).toBe(6);
  } finally {
    document.documentElement.style.removeProperty("--sb-bar-opacity");
    document.documentElement.style.removeProperty("--sb-bar-radius");
  }
});

test("whitespaceTokens keeps the bar-height floor in lockstep with the pad", () => {
  // Written in rem so the global size slider scales them; the numbers are the
  // same px values at 100%.
  expect(whitespaceTokens(BAR_PAD_Y_DEFAULT)).toEqual({
    "--sb-bar-pad-y": "0.375rem",
    "--sb-bar-height": "3.25rem",
  });
  expect(whitespaceTokens(4)).toEqual({
    "--sb-bar-pad-y": "0.25rem",
    "--sb-bar-height": "3rem",
  });
  // Out-of-range and non-finite input clamp / fall back.
  expect(whitespaceTokens(99)["--sb-bar-pad-y"]).toBe("1.25rem");
  expect(whitespaceTokens(Number.NaN)["--sb-bar-pad-y"]).toBe("0.375rem");
});

test("slider defaults mirror the bundled default theme tokens", () => {
  const theme = readThemeDocument(defaultTheme).tokens;
  expect(theme["--sb-tile-gap"]).toBe(pxToRem(GAP_DEFAULT));
  expect(theme["--sb-shortcut-gap"]).toBe(pxToRem(GAP_DEFAULT));
  expect(theme["--sb-bar-pad-y"]).toBe(pxToRem(BAR_PAD_Y_DEFAULT));
  expect(theme["--sb-bar-opacity"]).toBe(`${String(BAR_OPACITY_DEFAULT)}%`);
  expect(theme["--sb-flyout-opacity"]).toBe(
    `${String(FLYOUT_OPACITY_DEFAULT)}%`,
  );
  expect(theme["--sb-bar-radius"]).toBe(pxToRem(BAR_RADIUS_DEFAULT));
  expect(theme["--sb-bar-border-width"]).toBe(
    `${String(BAR_BORDER_DEFAULT)}px`,
  );
  expect(theme["--sb-bar-height"]).toBe(
    pxToRem(BAR_CONTENT_FLOOR_PX + 2 * BAR_PAD_Y_DEFAULT),
  );
});

test("slider readback is stable at every global scale", () => {
  // The trap this guards: reading a rem token against the LIVE root font size
  // made every slider show a different number per scale — and writing that
  // back would have frozen the value at one scale forever.
  const root = document.documentElement.style;
  try {
    for (const scale of ["0.75", "1", "1.5"]) {
      root.setProperty("--sb-scale", scale);
      root.setProperty("--sb-bar-radius", pxToRem(BAR_RADIUS_DEFAULT));
      expect(tokenNumber({}, "--sb-bar-radius", 0)).toBe(BAR_RADIUS_DEFAULT);
      expect(
        tokenNumber({ "--sb-tile-gap": pxToRem(6) }, "--sb-tile-gap", 0),
      ).toBe(6);
    }
  } finally {
    root.removeProperty("--sb-scale");
    root.removeProperty("--sb-bar-radius");
  }
});

test("colour token groups keep their semantic dependencies live", () => {
  expect(accentPrimaryTokens("#123456")).toEqual({
    "--sb-accent": "#123456",
    "--sb-accent-gradient":
      "linear-gradient(135deg, var(--sb-accent), var(--sb-accent-2))",
    "--sb-accent-glow":
      "0 2px 12px -2px color-mix(in srgb, var(--sb-accent) 35%, transparent)",
  });
  expect(accentSecondaryTokens("#abcdef")).toEqual({
    "--sb-accent-2": "#abcdef",
    "--sb-accent-gradient":
      "linear-gradient(135deg, var(--sb-accent), var(--sb-accent-2))",
  });
  expect(accentTokens("#123456", "#abcdef")).toEqual({
    "--sb-accent": "#123456",
    "--sb-accent-2": "#abcdef",
    "--sb-accent-gradient":
      "linear-gradient(135deg, var(--sb-accent), var(--sb-accent-2))",
    "--sb-accent-glow":
      "0 2px 12px -2px color-mix(in srgb, var(--sb-accent) 35%, transparent)",
  });

  const surfaces = surfaceTokens("#202020");
  expect(surfaces["--sb-bar-bg"]).toBe("#202020");
  expect(surfaces["--sb-tile-bg"]).toContain("var(--sb-text)");
  expect(surfaces["--sb-overlay-bg"]).toContain("var(--sb-bar-bg) 92%");
  expect(surfaces["--sb-tooltip-bg"]).toBe("var(--sb-flyout-bg)");

  expect(textTokens("#fefefe")).toMatchObject({
    "--sb-text": "#fefefe",
    "--sb-text-dim": "color-mix(in srgb, var(--sb-text) 74%, transparent)",
    "--sb-text-muted": "color-mix(in srgb, var(--sb-text) 62%, transparent)",
    "--sb-menu-text": "var(--sb-text)",
    "--sb-tooltip-text": "var(--sb-text)",
  });
});

test("reordering the visible tiles leaves hidden ones in their slots", () => {
  const all = ["clock", "cpu", "weather", "crypto"];
  const visible = ["clock", "weather", "crypto"];
  // "weather" (visible index 1) moves to the front of the visible row.
  expect(reorderWithHidden(all, visible, 1, 0)).toEqual([
    "weather",
    "cpu",
    "clock",
    "crypto",
  ]);
  // A no-op drop rewrites nothing.
  expect(reorderWithHidden(all, visible, 1, 1)).toEqual(all);
});
