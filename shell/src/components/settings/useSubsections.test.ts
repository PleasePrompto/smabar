// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { readSubsections, scrollToGroup } from "./useSubsections";

/**
 * The sub-navigation reads its entries out of the rendered section rather
 * than from a second, hand-kept list. That is the whole design decision, so
 * what is worth testing is the reading rule — the blocks it finds, and that
 * the index it reports is the one the click scrolls to.
 *
 * The observer half is not faked here: happy-dom's MutationObserver would
 * only prove the stub fires. `useSubsections` re-reads on every mutation,
 * which is the same `read()` these cases exercise.
 */
let host: HTMLElement;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
});
afterEach(() => {
  host.remove();
});

function block(title: string, rows = 1): string {
  const body = `<div class="settings-row">row</div>`.repeat(rows);
  return `<section class="settings-block"><h3 class="settings-block-title">${title}</h3><div class="settings-box">${body}</div></section>`;
}

test("every block of the open section becomes one entry", () => {
  // The tiles section has this shape: fixed blocks first, then one per
  // plugin that declares a schema — those arrive with `list_plugins`.
  host.innerHTML = block("Installed") + block("Weather") + block("Crypto");
  expect(readSubsections(host)).toEqual([
    { index: 0, label: "Installed" },
    { index: 1, label: "Weather" },
    { index: 2, label: "Crypto" },
  ]);

  const targets: string[] = [];
  for (const element of host.querySelectorAll(".settings-block")) {
    (element as HTMLElement).scrollIntoView = () => {
      targets.push(
        element.querySelector(".settings-block-title")?.textContent ?? "",
      );
    };
  }
  scrollToGroup(host, 2);
  expect(targets).toEqual(["Crypto"]);
});

test("a block whose title is empty cannot be navigated to", () => {
  host.innerHTML = block("") + block("Theme") + block("Colors");
  expect(readSubsections(host)).toEqual([
    { index: 1, label: "Theme" },
    { index: 2, label: "Colors" },
  ]);
});

test("one entry is not a navigation, so the section shows none", () => {
  host.innerHTML = block("Installed");
  expect(readSubsections(host)).toEqual([]);
  host.innerHTML = "";
  expect(readSubsections(host)).toEqual([]);
});

test("scrolling asks for the top of the block, and tolerates a stale index", () => {
  host.innerHTML = block("Theme");
  const spy = vi.fn();
  const first = host.querySelector<HTMLElement>(".settings-block");
  if (first === null) throw new Error("no block");
  first.scrollIntoView = spy;
  scrollToGroup(host, 0);
  expect(spy).toHaveBeenCalledWith({ block: "start", behavior: "smooth" });
  // The section can rebuild itself between reading an entry and clicking it;
  // that must be a no-op, not a crash.
  expect(() => {
    scrollToGroup(host, 9);
  }).not.toThrow();
  expect(() => {
    scrollToGroup(null, 0);
  }).not.toThrow();
});

test("navigating to a tile card opens its settings without nesting nav entries", () => {
  host.innerHTML =
    block("Installed") +
    `<section class="settings-block settings-plugin-card">
    <div class="settings-box"><h3 class="settings-block-title">Clock</h3>
    <details class="settings-plugin-details"><summary>Settings &amp; audio</summary><input></details></div>
    </section>`;
  const card = host.querySelector<HTMLElement>(".settings-plugin-card");
  const details = host.querySelector<HTMLDetailsElement>("details");
  if (card === null || details === null) throw new Error("Missing card");
  const scroll = vi.fn();
  card.scrollIntoView = scroll;
  expect(readSubsections(host).map((entry) => entry.label)).toEqual([
    "Installed",
    "Clock",
  ]);
  scrollToGroup(host, 1);
  expect(details.open).toBe(true);
  expect(scroll).toHaveBeenCalledOnce();
});
