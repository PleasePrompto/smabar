// @vitest-environment happy-dom
import { expect, test } from "vitest";

import defaultTheme from "../../../themes/default.json";
import paperTheme from "../../../themes/paper.json";
import terminalTheme from "../../../themes/terminal.json";
import topbarTheme from "../../../themes/topbar.json";
import {
  colorToHex,
  colorsEqual,
  contrastRatio,
  contrastText,
  minimumContrast,
  parseColor,
  readableTextOn,
  readableTextOnAll,
  relativeLuminance,
} from "./contrast";

test("parseColor reads the CSS colour forms themes actually use", () => {
  expect(parseColor("#fff")).toEqual([255, 255, 255]);
  expect(parseColor("#141626")).toEqual([20, 22, 38]);
  expect(parseColor("#141626ff")).toEqual([20, 22, 38]);
  expect(parseColor("rgb(20, 22, 38)")).toEqual([20, 22, 38]);
  expect(parseColor("rgba(20, 22, 38, 0.55)")).toEqual([20, 22, 38]);
  expect(parseColor("rgb(100% 100% 100%)")).toBeNull();
  expect(parseColor("  #FAF8F5  ")).toEqual([250, 248, 245]);
  // No opinion on things we cannot measure — the declared value then stands.
  expect(parseColor("linear-gradient(135deg, #000, #fff)")).toBeNull();
  expect(parseColor("var(--sb-accent)")).toBeNull();
  expect(parseColor("oklch(76.5% 0.177 163.223)")).toBeNull();
  expect(parseColor("")).toBeNull();
});

test("luminance and ratio match the WCAG reference values", () => {
  expect(relativeLuminance([0, 0, 0])).toBe(0);
  expect(relativeLuminance([255, 255, 255])).toBeCloseTo(1, 5);
  expect(contrastRatio([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 2);
  expect(contrastRatio([255, 255, 255], [255, 255, 255])).toBeCloseTo(1, 5);
});

test("contrastText picks the readable end for light and dark surfaces", () => {
  expect(contrastText("#141626")).toBe("#ffffff");
  expect(contrastText("#faf8f5")).toBe("#09060f");
  // The exact failure the user reported: a pale brand colour.
  expect(contrastText("#fcd34d")).toBe("#09060f");
  expect(contrastText("nonsense")).toBeNull();
});

test("readableTextOn only overrides a declared colour that fails", () => {
  // Legible pairs are left alone (null = keep what the author declared).
  expect(readableTextOn("#141626", "#ffffff")).toBeNull();
  expect(readableTextOn("#faf8f5", "#1c1917")).toBeNull();
  // White on a pale accent is the bug — it gets corrected.
  expect(readableTextOn("#fcd34d", "#ffffff")).toBe("#09060f"); // 1.44:1
  expect(readableTextOn("#f59e0b", "#ffffff")).toBe("#09060f"); // 2.15:1
  // Normal-sized labels below 4.5:1 are corrected as well.
  expect(readableTextOn("#8b5cf6", "#ffffff")).toBe("#09060f"); // 4.23:1
  expect(readableTextOn("#3b82f6", "#ffffff")).toBe("#09060f"); // 3.68:1
  // Missing declaration still yields a usable colour.
  expect(readableTextOn("#0f766e", undefined)).toBe("#ffffff");
  // Nothing measurable, nothing to say.
  expect(readableTextOn(undefined, "#fff")).toBeNull();
  expect(readableTextOn("var(--x)", "#fff")).toBeNull();
});

test("colour identity is semantic and never invents a black fallback", () => {
  expect(colorToHex("#fff")).toBe("#ffffff");
  expect(colorToHex("#F59E0B")).toBe("#f59e0b");
  expect(colorToHex("not-a-colour")).toBeNull();
  expect(colorToHex("var(--missing)")).toBeNull();
  expect(colorToHex("currentColor")).toBeNull();
  expect(colorsEqual("#fff", "#ffffff")).toBe(true);
  expect(colorsEqual(" rgb(255, 255, 255) ", "#ffffff")).toBe(true);
  expect(colorsEqual("var(--missing)", "VAR(--MISSING)")).toBe(true);
});

test("one foreground is checked against both ends of an accent gradient", () => {
  expect(readableTextOnAll(["#8b5cf6", "#fcd34d"], "#ffffff")).toBe("#09060f");
  expect(readableTextOnAll(["#141626", "#1e3a8a"], "#ffffff")).toBeNull();
  expect(
    minimumContrast(["#8b5cf6", "#fcd34d"], "#09060f"),
  ).toBeGreaterThanOrEqual(4.5);
  expect(minimumContrast(["#8b5cf6", "nonsense"], "#fff")).toBeNull();
});

test("every bundled theme already pairs its text with its surfaces", () => {
  const themes = {
    default: defaultTheme,
    paper: paperTheme,
    terminal: terminalTheme,
    topbar: topbarTheme,
  };
  for (const [name, theme] of Object.entries(themes)) {
    // No bundled theme should ever trigger the correction: if one does, the
    // theme file is wrong, not the helper.
    expect(
      readableTextOn(theme["--sb-bar-bg"], theme["--sb-text"]),
      `${name}: bar text`,
    ).toBeNull();
    expect(
      readableTextOn(theme["--sb-flyout-bg"], theme["--sb-text"]),
      `${name}: flyout text`,
    ).toBeNull();
    expect(
      Number.parseFloat(theme["--sb-flyout-opacity"]),
      `${name}: flyout opacity`,
    ).toBeGreaterThanOrEqual(90);
    expect(
      readableTextOn(theme["--sb-accent"], theme["--sb-on-accent"]),
      `${name}: accent text`,
    ).toBeNull();
    expect(
      readableTextOn(theme["--sb-accent-2"], theme["--sb-on-accent"]),
      `${name}: secondary accent text`,
    ).toBeNull();
  }
});
