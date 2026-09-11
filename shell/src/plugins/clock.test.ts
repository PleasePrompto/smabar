// @vitest-environment happy-dom
import { expect, test, vi } from "vitest";

import {
  clockText,
  enhanceClocks,
  isoWeek,
  secondsSinceMidnight,
  utcOffset,
  zonedParts,
} from "./clock";

/** A fixed instant: 2026-08-23T05:48:17Z (a Sunday). */
const AT = new Date("2026-08-23T05:48:17.000Z");

test("wall-clock parts follow the requested zone", () => {
  expect(zonedParts(AT, "UTC")).toEqual({ hours: 5, minutes: 48, seconds: 17 });
  // Berlin is UTC+2 in August (CEST).
  expect(zonedParts(AT, "Europe/Berlin")).toEqual({
    hours: 7,
    minutes: 48,
    seconds: 17,
  });
  // Tokyo is UTC+9 all year.
  expect(zonedParts(AT, "Asia/Tokyo")).toEqual({
    hours: 14,
    minutes: 48,
    seconds: 17,
  });
  // An unusable zone must not throw — it falls back to the system zone.
  expect(() => zonedParts(AT, "Mars/Olympus")).not.toThrow();
});

test("reuses an Intl formatter across clock ticks", () => {
  const descriptor = Object.getOwnPropertyDescriptor(Intl, "DateTimeFormat");
  const original = Intl.DateTimeFormat;
  let constructions = 0;
  class TrackedDateTimeFormat extends original {
    constructor(
      locales?: Intl.LocalesArgument,
      options?: Intl.DateTimeFormatOptions,
    ) {
      constructions += 1;
      super(locales, options);
    }
  }
  Object.defineProperty(Intl, "DateTimeFormat", {
    configurable: true,
    value: TrackedDateTimeFormat,
  });
  try {
    zonedParts(AT, "Pacific/Chatham");
    expect(constructions).toBe(1);
    zonedParts(AT, "Pacific/Chatham");
    expect(constructions).toBe(1);
  } finally {
    if (descriptor !== undefined) {
      Object.defineProperty(Intl, "DateTimeFormat", descriptor);
    }
  }
});

test("the system-zone clock follows an OS timezone change", () => {
  const previous = process.env.TZ;
  try {
    process.env.TZ = "UTC";
    expect(clockText(AT, "", "time", "en")).toBe("05:48:17");
    process.env.TZ = "Asia/Tokyo";
    expect(clockText(AT, "", "time", "en")).toBe("14:48:17");
  } finally {
    if (previous === undefined) delete process.env.TZ;
    else process.env.TZ = previous;
  }
});

test("seconds since midnight drive the hand offsets", () => {
  expect(secondsSinceMidnight(AT, "UTC")).toBe(5 * 3600 + 48 * 60 + 17);
  expect(secondsSinceMidnight(AT, "Europe/Berlin")).toBe(
    7 * 3600 + 48 * 60 + 17,
  );
});

test("ISO week numbers follow the Thursday rule", () => {
  expect(isoWeek(AT, "UTC")).toBe(34);
  // 2027-01-01 is a Friday, so it still belongs to week 53 of 2026.
  expect(isoWeek(new Date("2027-01-01T12:00:00.000Z"), "UTC")).toBe(53);
  // 2026-01-01 is a Thursday: week 1.
  expect(isoWeek(new Date("2026-01-01T12:00:00.000Z"), "UTC")).toBe(1);
  // The zone decides which calendar day it is.
  expect(isoWeek(new Date("2026-01-04T23:30:00.000Z"), "Asia/Tokyo")).toBe(2);
});

test("UTC offsets render in their shortest exact form", () => {
  expect(utcOffset(AT, "UTC")).toBe("UTC±0");
  expect(utcOffset(AT, "Europe/Berlin")).toBe("UTC+2");
  expect(utcOffset(AT, "Asia/Kolkata")).toBe("UTC+5:30");
  expect(utcOffset(AT, "America/New_York")).toBe("UTC−4");
});

test("clockText renders each format in the requested zone", () => {
  expect(clockText(AT, "Europe/Berlin", "time", "en")).toBe("07:48:17");
  expect(clockText(AT, "Europe/Berlin", "time", "en", false)).toBe("07:48");
  expect(clockText(AT, "Asia/Tokyo", "time", "en")).toBe("14:48:17");
  expect(clockText(AT, "Europe/Berlin", "weekday", "en")).toBe("Sunday");
  expect(clockText(AT, "Europe/Berlin", "week", "en")).toBe("34");
  expect(clockText(AT, "Europe/Berlin", "offset", "en")).toBe("UTC+2");
  // Unix time is zone-independent — the same instant in every zone.
  expect(clockText(AT, "Asia/Tokyo", "unix", "en")).toBe("1787464097");
  expect(clockText(AT, "Europe/Berlin", "date", "de")).toBe("23. August");
});

test("the enhancer fills text nodes and points the hands at the time", () => {
  document.body.innerHTML =
    '<div><span data-clock-text="Europe/Berlin" data-clock-format="time"></span>' +
    '<span data-clock-text="Asia/Tokyo" data-clock-format="offset"></span>' +
    '<div data-clock="Europe/Berlin"></div></div>';
  const root = document.body.firstElementChild;
  if (root === null) throw new Error("fixture missing");

  const stop = enhanceClocks(root, () => AT);
  try {
    const [time, offset] = [...root.querySelectorAll("[data-clock-text]")];
    expect(time?.textContent).toBe("07:48:17");
    expect(offset?.textContent).toBe("UTC+9");

    // The face is built from real SVG nodes (post-sanitize, like the charts).
    const face = root.querySelector("svg.sb-clock-face");
    expect(face).not.toBeNull();
    expect(root.querySelectorAll(".sb-clock-tick")).toHaveLength(12);

    // Berlin is 07:48:17 → the hands start that far into their periods.
    const delay = (selector: string) =>
      root.querySelector<SVGElement>(selector)?.style.animationDelay;
    const since = 7 * 3600 + 48 * 60 + 17;
    expect(delay(".sb-clock-second")).toBe(`-${String(since % 60)}s`);
    expect(delay(".sb-clock-minute")).toBe(`-${String(since % 3600)}s`);
    expect(delay(".sb-clock-hour")).toBe(`-${String(since % 43200)}s`);
  } finally {
    stop();
  }
});

/** A `matchMedia` answer with a fixed `matches`; the listeners are inert. */
function motionQuery(matches: boolean): MediaQueryList {
  return {
    matches,
    media: "(prefers-reduced-motion: reduce)",
    onchange: null,
    addListener: () => undefined,
    removeListener: () => undefined,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    dispatchEvent: () => true,
  };
}

/** Second-hand offset of the face under `root`, as the kit animation sees it. */
function secondHandDelay(root: Element): string | undefined {
  return root.querySelector<SVGElement>(".sb-clock-second")?.style
    .animationDelay;
}

test("under reduced motion the paused hands are re-seeded every second", () => {
  // The kit pauses the hand animation for reduced motion, so a face that is
  // seeded once would show its start time forever (Windows RDP sessions
  // report reduced motion, which is how this surfaced as a stuck clock).
  vi.useFakeTimers();
  const matchMedia = vi
    .spyOn(window, "matchMedia")
    .mockReturnValue(motionQuery(true));
  document.body.innerHTML = '<div><div data-clock="Europe/Berlin"></div></div>';
  const root = document.body.firstElementChild;
  if (root === null) throw new Error("fixture missing");
  let current = AT;
  const stop = enhanceClocks(root, () => current);
  try {
    expect(secondHandDelay(root)).toBe("-17s");
    current = new Date(AT.getTime() + 1000);
    vi.advanceTimersByTime(1000);
    expect(secondHandDelay(root)).toBe("-18s");
  } finally {
    stop();
    matchMedia.mockRestore();
    vi.useRealTimers();
  }
});

test("without reduced motion the sweeping hands keep their one-time seed", () => {
  vi.useFakeTimers();
  const matchMedia = vi
    .spyOn(window, "matchMedia")
    .mockReturnValue(motionQuery(false));
  document.body.innerHTML = '<div><div data-clock="Europe/Berlin"></div></div>';
  const root = document.body.firstElementChild;
  if (root === null) throw new Error("fixture missing");
  let current = AT;
  const stop = enhanceClocks(root, () => current);
  try {
    current = new Date(AT.getTime() + 1000);
    vi.advanceTimersByTime(1000);
    // The CSS animation carries the sweep; JS must not fight it.
    expect(secondHandDelay(root)).toBe("-17s");
  } finally {
    stop();
    matchMedia.mockRestore();
    vi.useRealTimers();
  }
});

test("stopping the enhancer ends the tick", () => {
  document.body.innerHTML = '<div><span data-clock-text=""></span></div>';
  const root = document.body.firstElementChild;
  if (root === null) throw new Error("fixture missing");
  const stop = enhanceClocks(root, () => AT);
  const before = root.querySelector("span")?.textContent;
  expect(before).not.toBe("");
  // No pending timer may survive: ShadowHost tears widgets down on every push.
  expect(() => {
    stop();
  }).not.toThrow();
});
