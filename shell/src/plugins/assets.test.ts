// @vitest-environment happy-dom
// convertFileSrc reads window.__TAURI_INTERNALS__, so it needs a DOM.
import { expect, test } from "vitest";

import { assetRelativePath, resolveAsset, setAssetRoot } from "./assets";

test("the scheme is inert until the data root arrives", () => {
  setAssetRoot("");
  expect(resolveAsset("clock", "sb-asset:logo.png")).toBeNull();
});

test("a resolved asset stays inside the plugin's own directory", () => {
  setAssetRoot("/home/u/.smabar/data");
  // Stand in for the Tauri window; convertFileSrc reads this.
  const asWindow = window as unknown as Record<string, unknown>;
  asWindow.__TAURI_INTERNALS__ = {
    convertFileSrc: (path: string) => `asset://localhost/${path}`,
  };
  try {
    expect(resolveAsset("clock", "sb-asset:icons/sun.svg")).toBe(
      "asset://localhost//home/u/.smabar/data/clock/icons/sun.svg",
    );
    expect(resolveAsset("clock", "sb-asset:../other/secret")).toBeNull();
  } finally {
    delete asWindow.__TAURI_INTERNALS__;
  }
});

test("outside the Tauri window the scheme resolves to nothing", () => {
  setAssetRoot("/home/u/.smabar/data");
  expect(resolveAsset("clock", "sb-asset:logo.png")).toBeNull();
});

test("an unknown plugin resolves to nothing", () => {
  setAssetRoot("/home/u/.smabar/data");
  expect(resolveAsset(undefined, "sb-asset:logo.png")).toBeNull();
  expect(resolveAsset("", "sb-asset:logo.png")).toBeNull();
});

test("only the sb-asset scheme is claimed", () => {
  expect(assetRelativePath("https://example.com/a.png")).toBeNull();
  expect(assetRelativePath("data:image/png;base64,AA")).toBeNull();
  expect(assetRelativePath("sb-asset:a.png")).toBe("a.png");
});

test("backslash separators are normalised, not a way around the check", () => {
  expect(assetRelativePath("sb-asset:icons\\sun.svg")).toBe("icons/sun.svg");
  expect(assetRelativePath("sb-asset:icons\\..\\..\\x")).toBeNull();
});
