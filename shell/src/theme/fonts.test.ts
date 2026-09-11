// @vitest-environment happy-dom
import { beforeEach, expect, test, vi } from "vitest";

import type { EnsuredGoogleFont } from "./fonts";
import {
  canonicalizeManagedFonts,
  fontFamilyStack,
  googleFontId,
  syncManagedThemeFonts,
} from "./fonts";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../ipc/call", () => ({ call: callMock }));

beforeEach(() => {
  callMock.mockReset();
  document.documentElement.removeAttribute("style");
});

test("font token helpers keep portable fallbacks and strict Google ids", () => {
  expect(fontFamilyStack("Noto Sans", "sans")).toBe('"Noto Sans", sans-serif');
  expect(fontFamilyStack("monospace", "mono")).toBe("monospace");
  expect(fontFamilyStack("ui-monospace", "mono")).toBe("ui-monospace");
  expect(fontFamilyStack("Fraunces", "sans", "serif")).toBe(
    '"Fraunces", serif',
  );
  expect(googleFontId("google:noto-sans")).toBe("noto-sans");
  expect(googleFontId("system")).toBeNull();
});

test("the catalog id corrects a foreign theme family without a stale race", async () => {
  const pending = new Map<string, (font: EnsuredGoogleFont) => void>();
  callMock.mockImplementation(
    (_command: string, args?: Record<string, unknown>) =>
      new Promise<EnsuredGoogleFont>((resolve) => {
        if (typeof args?.id === "string") pending.set(args.id, resolve);
      }),
  );

  syncManagedThemeFonts({ "--sb-font-sans-source": "google:first" });
  syncManagedThemeFonts({ "--sb-font-sans-source": "google:second" });
  pending.get("first")?.({ id: "first", family: "First", faces: [] });
  await Promise.resolve();
  expect(
    document.documentElement.style.getPropertyValue("--sb-font-sans"),
  ).toBe("");

  pending.get("second")?.({ id: "second", family: "Canonical", faces: [] });
  await vi.waitFor(() => {
    expect(
      document.documentElement.style.getPropertyValue("--sb-font-sans"),
    ).toBe('"Canonical", sans-serif');
  });
  expect(
    canonicalizeManagedFonts({
      "--sb-font-sans": "Wrong, serif",
      "--sb-font-sans-source": "google:second",
    })["--sb-font-sans"],
  ).toBe('"Canonical", serif');
});
