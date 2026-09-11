// @vitest-environment happy-dom
import { beforeEach, expect, test, vi } from "vitest";

import { applyTheme, applyTokenOverrides } from "./apply";
import { readThemeDocument } from "./document";

const rootStyle = () => document.documentElement.style;

beforeEach(() => {
  // Reset module state and the root element between tests.
  applyTheme({});
  applyTokenOverrides({});
  document.documentElement.removeAttribute("style");
});

test("sets tokens as custom properties on the document root", () => {
  applyTheme({ "--sb-accent": "#00ff88", "--sb-radius-m": "1rem" });
  expect(rootStyle().getPropertyValue("--sb-accent")).toBe("#00ff88");
  expect(rootStyle().getPropertyValue("--sb-radius-m")).toBe("1rem");
});

test("does not rewrite unchanged root tokens", () => {
  applyTheme({
    "--sb-accent": "#1e3a8a",
    "--sb-accent-2": "#1e3a8a",
    "--sb-on-accent": "#ffffff",
    "--sb-bar-bg": "#141626",
    "--sb-text": "#ffffff",
    "--sb-radius-m": "1rem",
  });
  const setProperty = vi.spyOn(rootStyle(), "setProperty");

  applyTokenOverrides({ "--sb-radius-m": "2rem" });
  expect(setProperty).toHaveBeenCalledOnce();
  expect(setProperty).toHaveBeenCalledWith("--sb-radius-m", "2rem");

  applyTokenOverrides({ "--sb-radius-m": "2rem" });
  expect(setProperty).toHaveBeenCalledOnce();
  setProperty.mockRestore();
});

test("theme documents keep settings out of the CSS token layer", () => {
  expect(
    readThemeDocument({
      "--sb-accent": "#00ff88",
      settings: { "layout.position": "top" },
    }),
  ).toEqual({
    tokens: { "--sb-accent": "#00ff88" },
    settings: { "layout.position": "top" },
  });
});

test("removes previously applied tokens that are missing from the next theme", () => {
  applyTheme({ "--sb-accent": "#00ff88", "--my-plugin-glow": "0 0 8px red" });
  applyTheme({ "--sb-accent": "#ff0088" });
  expect(rootStyle().getPropertyValue("--sb-accent")).toBe("#ff0088");
  expect(rootStyle().getPropertyValue("--my-plugin-glow")).toBe("");
});

test("ignores keys that are not custom-property names", () => {
  applyTheme({ background: "red", "--sb-accent": "#00ff88" });
  expect(rootStyle().getPropertyValue("background")).toBe("");
  expect(rootStyle().getPropertyValue("--sb-accent")).toBe("#00ff88");
});

test("applying an empty theme clears everything it set before", () => {
  applyTheme({ "--sb-accent": "#00ff88" });
  applyTheme({});
  expect(rootStyle().getPropertyValue("--sb-accent")).toBe("");
});

test("overrides win over the theme and survive a theme switch", () => {
  applyTheme({ "--sb-accent": "#00ff88", "--sb-bar-gap": "6px" });
  applyTokenOverrides({ "--sb-bar-gap": "12px" });
  expect(rootStyle().getPropertyValue("--sb-bar-gap")).toBe("12px");
  expect(rootStyle().getPropertyValue("--sb-accent")).toBe("#00ff88");

  applyTheme({ "--sb-accent": "#ff0088" });
  expect(rootStyle().getPropertyValue("--sb-bar-gap")).toBe("12px");
  expect(rootStyle().getPropertyValue("--sb-accent")).toBe("#ff0088");
});

test("clearing the overrides falls back to the theme value", () => {
  applyTheme({ "--sb-bar-gap": "6px" });
  applyTokenOverrides({ "--sb-bar-gap": "12px", "--sb-bar-opacity": "50%" });
  applyTokenOverrides({});
  expect(rootStyle().getPropertyValue("--sb-bar-gap")).toBe("6px");
  // Tokens only ever set by the override layer are removed entirely.
  expect(rootStyle().getPropertyValue("--sb-bar-opacity")).toBe("");
});

test("unreadable text tokens are repaired, legible ones are left alone", () => {
  // The bundled pairing is legible and must survive untouched.
  applyTheme({
    "--sb-accent": "#8b5cf6",
    "--sb-on-accent": "#09060f",
    "--sb-bar-bg": "#141626",
    "--sb-text": "#ffffff",
  });
  expect(rootStyle().getPropertyValue("--sb-on-accent")).toBe("#09060f");
  expect(rootStyle().getPropertyValue("--sb-text")).toBe("#ffffff");

  // Picking a pale accent must not leave white text on it. This is the
  // whole point: every accent surface in the kit reads --sb-on-accent.
  applyTokenOverrides({ "--sb-accent": "#fcd34d" });
  expect(rootStyle().getPropertyValue("--sb-on-accent")).toBe("#09060f");

  // …and going back to a dark accent restores white.
  applyTokenOverrides({ "--sb-accent": "#1e3a8a" });
  expect(rootStyle().getPropertyValue("--sb-on-accent")).toBe("#ffffff");
});

test("accent text must work on both gradient endpoints", () => {
  applyTheme({
    "--sb-accent": "#8b5cf6",
    "--sb-accent-2": "#fcd34d",
    "--sb-on-accent": "#ffffff",
  });
  expect(rootStyle().getPropertyValue("--sb-on-accent")).toBe("#09060f");
});

test("a picked text colour is honoured, an unpicked one follows the surface", () => {
  applyTheme({ "--sb-bar-bg": "#141626", "--sb-text": "#ffffff" });

  // A light bar background with the theme's white text would be unreadable,
  // so the text follows the surface.
  applyTokenOverrides({ "--sb-bar-bg": "#faf8f5" });
  expect(rootStyle().getPropertyValue("--sb-text")).toBe("#09060f");

  // An explicitly picked text colour is an instruction, not a suggestion —
  // even a poor one stands.
  applyTokenOverrides({
    "--sb-bar-bg": "#faf8f5",
    "--sb-text": "#eeeeee",
  });
  expect(rootStyle().getPropertyValue("--sb-text")).toBe("#eeeeee");
});
