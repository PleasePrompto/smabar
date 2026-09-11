// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";

import {
  logWebviewCapabilities,
  requiredMissing,
  webviewCapabilities,
} from "./capabilities";

const logged: { level: string; message: string; options?: unknown }[] = [];
vi.mock("./log", () => ({
  uiLog: (level: string, message: string, options?: unknown) => {
    logged.push({ level, message, options });
  },
}));

test("every probe answers with a boolean and never throws", () => {
  // happy-dom implements almost none of this; the point is that an
  // unsupported feature reports false instead of taking the bar down.
  const caps = webviewCapabilities();
  expect(Object.keys(caps).length).toBeGreaterThan(10);
  for (const [name, value] of Object.entries(caps)) {
    expect(typeof value, name).toBe("boolean");
  }
});

test("the patterns the plugin guide documents are all probed", () => {
  // If the guide tells authors to use <dialog> + commandfor and the runtime
  // cannot, the log has to say so — so these names must exist.
  const caps = webviewCapabilities();
  for (const name of [
    "dialogOpensFromInvoker",
    "popoverOpensFromTrigger",
    "detailsToggles",
    "registeredProperty",
  ]) {
    expect(caps, name).toHaveProperty(name);
  }
});

test("the report names what is missing, not just that something is", () => {
  logged.length = 0;
  logWebviewCapabilities();
  expect(logged).toHaveLength(1);
  expect(logged[0]?.level).toBe("info");
  // happy-dom is missing plenty, so this run must list names.
  expect(logged[0]?.message).toContain("MISSING");
});

test("probing leaves no scratch element behind", () => {
  const before = document.body.childElementCount;
  webviewCapabilities();
  expect(document.body.childElementCount).toBe(before);
});

test("a Chrome-only feature the kit avoids is not reported as missing", () => {
  // WebKitGTK answers false to both of these, and that is the expected
  // answer: the kit is built without them on purpose. Reporting it as a
  // defect sent the user hunting for a fix that does not exist.
  expect(
    requiredMissing({ interpolateSize: false, fieldSizing: false }),
  ).toEqual([]);
  // Anything the kit does rely on still gets named.
  expect(
    requiredMissing({ interpolateSize: false, oklch: false, popover: true }),
  ).toEqual(["oklch"]);
});
