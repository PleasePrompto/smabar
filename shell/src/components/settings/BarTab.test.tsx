// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test } from "vitest";

import { useSmabar, type LayoutConfig } from "../../store/bar";
import { BarTab } from "./BarTab";

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  useSmabar.setState(useSmabar.getInitialState(), true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  act(() => {
    root.render(<BarTab />);
  });
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
});

function setLayout(patch: Partial<LayoutConfig>) {
  act(() => {
    const state = useSmabar.getState();
    state.setLayout({ ...state.layout, ...patch });
  });
}

function reveal(label: string): HTMLElement {
  const row = [
    ...container.querySelectorAll<HTMLElement>(".settings-row"),
  ].find(
    (candidate) =>
      candidate.querySelector(".settings-row-label")?.textContent === label,
  );
  const result = row?.closest<HTMLElement>(".settings-reveal");
  if (result === null || result === undefined) {
    throw new Error(`No reveal found for ${label}`);
  }
  return result;
}

function expectVisible(label: string, visible: boolean) {
  const row = reveal(label);
  expect(row.classList.contains("sb-active"), label).toBe(visible);
  expect(row.hasAttribute("inert"), label).toBe(!visible);
  expect(row.getAttribute("aria-hidden"), label).toBe(visible ? null : "true");
}

test("layout controls reveal only where the selected layout supports them", () => {
  const primary = reveal("Primary zone").querySelector(".settings-row");
  expectVisible("Primary zone", false);
  expectVisible("Limit full width", true);
  expectVisible("Window stacking", false);

  setLayout({ variant: "rows", width: "full" });
  expect(reveal("Primary zone").querySelector(".settings-row")).toBe(primary);
  expectVisible("Primary zone", true);
  expectVisible("Limit full width", true);

  setLayout({ variant: "solo", width: "auto", behavior: "float" });
  expectVisible("Primary zone", true);
  expectVisible("Limit full width", false);
  expectVisible("Window stacking", true);
});
