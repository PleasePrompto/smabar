import { beforeEach, expect, test } from "vitest";

import { safeIntlLocale, setLocale, t } from "./t";

beforeEach(() => {
  setLocale({});
});

test("resolves a key from the bundled English locale", () => {
  expect(t("app.hello")).toBe("Hello smabar");
});

test("prefers the active drop-in locale over English", () => {
  setLocale({ "app.hello": "Hallo smabar" });
  expect(t("app.hello")).toBe("Hallo smabar");
});

test("falls back to English for keys missing in the active locale", () => {
  setLocale({ unrelated: "x" });
  expect(t("app.hello")).toBe("Hello smabar");
});

test("returns the key itself when no locale knows it", () => {
  expect(t("does.not.exist")).toBe("does.not.exist");
});

test("safeIntlLocale accepts bundled languages and rejects invalid codes", () => {
  expect(safeIntlLocale("en")).toBe("en");
  expect(safeIntlLocale("de")).toBe("de");
  expect(safeIntlLocale("../evil")).toBe("en");
  expect(safeIntlLocale("zz")).toBe("en");
});
