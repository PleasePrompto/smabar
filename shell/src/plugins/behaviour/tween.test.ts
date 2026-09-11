// @vitest-environment happy-dom
/**
 * data-sb-tween: numeric count-up across renders, driven by the shell-owned
 * data-sb-scope value memory. rAF is stubbed so the frames run when the test
 * says so, with explicit timestamps against the 240ms fallback duration.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { syncKit } from "./delegate";
import { clearTweenMemory } from "./tween";

let frames: FrameRequestCallback[] = [];

beforeEach(() => {
  frames = [];
  vi.stubGlobal(
    "requestAnimationFrame",
    (callback: FrameRequestCallback): number => frames.push(callback),
  );
  vi.spyOn(performance, "now").mockReturnValue(0);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/** Runs every queued frame once with the given timestamp. */
function flush(now: number): void {
  const queued = frames;
  frames = [];
  for (const callback of queued) callback(now);
}

function render(scope: string | null, html: string): HTMLElement {
  const root = document.createElement("div");
  if (scope !== null) root.dataset.sbScope = scope;
  root.innerHTML = html;
  document.body.appendChild(root);
  syncKit(root);
  return root;
}

function text(root: HTMLElement): string {
  return root.querySelector("[data-sb-tween]")?.textContent ?? "";
}

function texts(root: HTMLElement): string[] {
  return [...root.querySelectorAll("[data-sb-tween]")].map(
    (element) => element.textContent,
  );
}

test("the first render shows the target directly", () => {
  const root = render("s1", "<span data-sb-tween>48 min</span>");
  expect(text(root)).toBe("48 min");
  expect(frames).toHaveLength(0);
});

test("a changed value counts from the old value to the exact new text", () => {
  render("s2", "<span data-sb-tween>20 min</span>");
  const root = render("s2", "<span data-sb-tween>60 min</span>");
  // The old value paints synchronously — no target flash before the count.
  expect(text(root)).toBe("20 min");
  // Halfway through the 240ms fallback: cubic ease-out has covered 87.5%.
  flush(120);
  expect(text(root)).toBe("55 min");
  // The end restores the exact rendered text, not a re-format.
  flush(240);
  expect(text(root)).toBe("60 min");
  expect(frames).toHaveLength(0);
});

test("prefix, suffix and the decimal format of the new text are kept", () => {
  render("s3", "<span data-sb-tween>€ 1,20</span>");
  const root = render("s3", "<span data-sb-tween>€ 1,80</span>");
  expect(text(root)).toBe("€ 1,20");
  flush(240);
  expect(text(root)).toBe("€ 1,80");
});

test("reduced motion shows the target immediately", () => {
  const media = vi
    .spyOn(window, "matchMedia")
    .mockReturnValue({ matches: true } as MediaQueryList);
  render("s4", "<span data-sb-tween>10</span>");
  const root = render("s4", "<span data-sb-tween>90</span>");
  expect(text(root)).toBe("90");
  expect(frames).toHaveLength(0);
  media.mockRestore();
});

test("a zero theme duration shows the target immediately", () => {
  render("s-zero", '<span data-sb-tween style="--sb-dur-slow: 0ms">10</span>');
  const root = render(
    "s-zero",
    '<span data-sb-tween style="--sb-dur-slow: 0ms">20</span>',
  );
  expect(text(root)).toBe("20");
  expect(frames).toHaveLength(0);
});

test("without a scope the value stands as rendered", () => {
  const root = render(null, "<span data-sb-tween>42</span>");
  expect(text(root)).toBe("42");
  expect(frames).toHaveLength(0);
});

test("stable keys prevent reordered values from inheriting their neighbor", () => {
  render(
    "s5",
    '<span data-sb-tween data-sb-key="a">10</span>' +
      '<span data-sb-tween data-sb-key="b">20</span>',
  );
  const reordered = render(
    "s5",
    '<span data-sb-tween data-sb-key="b">40</span>' +
      '<span data-sb-tween data-sb-key="a">30</span>',
  );
  expect(texts(reordered)).toEqual(["20", "10"]);
});

test("multiple unkeyed values do not borrow index-based animation state", () => {
  render("s6", "<span data-sb-tween>10</span><span data-sb-tween>20</span>");
  const reordered = render(
    "s6",
    "<span data-sb-tween>40</span><span data-sb-tween>30</span>",
  );
  expect(texts(reordered)).toEqual(["40", "30"]);
});

test('the public key "only" cannot inherit the single-element fallback', () => {
  render("s-only", "<span data-sb-tween>10</span>");
  const keyed = render(
    "s-only",
    '<span data-sb-tween data-sb-key="only">90</span>' +
      '<span data-sb-tween data-sb-key="other">20</span>',
  );
  expect(texts(keyed)).toEqual(["90", "20"]);
  expect(frames).toHaveLength(0);
});

test("excessive decimal precision is left unchanged instead of throwing", () => {
  const exact = `1.${"2".repeat(101)}`;
  render("s-precision", "<span data-sb-tween>1.0</span>");
  const root = render("s-precision", `<span data-sb-tween>${exact}</span>`);
  expect(text(root)).toBe(exact);
  expect(frames).toHaveLength(0);
});

test("an interrupted tween resumes from its last painted value", () => {
  render("s7", '<span data-sb-tween data-sb-key="value">0</span>');
  render("s7", '<span data-sb-tween data-sb-key="value">100</span>');
  flush(120);
  const next = render(
    "s7",
    '<span data-sb-tween data-sb-key="value">200</span>',
  );
  expect(text(next)).toBe("88");
  flush(240);
  expect(text(next)).toBe("200");
});

test("removed keys do not leak a previous value when reinserted", () => {
  render(
    "s8",
    '<span data-sb-tween data-sb-key="a">10</span>' +
      '<span data-sb-tween data-sb-key="b">20</span>',
  );
  render("s8", '<span data-sb-tween data-sb-key="a">11</span>');
  const reinserted = render(
    "s8",
    '<span data-sb-tween data-sb-key="b">99</span>',
  );
  expect(text(reinserted)).toBe("99");
});

test("a stale tween frame cannot reuse a remounted scope revision", () => {
  render("s9", '<span data-sb-tween data-sb-key="value">0</span>');
  render("s9", '<span data-sb-tween data-sb-key="value">100</span>');
  const stale = frames.shift();

  clearTweenMemory("s9");
  render("s9", '<span data-sb-tween data-sb-key="value">80</span>');
  render("s9", '<span data-sb-tween data-sb-key="value">160</span>');
  stale?.(120);

  const following = render(
    "s9",
    '<span data-sb-tween data-sb-key="value">200</span>',
  );
  expect(text(following)).toBe("80");
});
