// @vitest-environment happy-dom
/** Shared state semantics: dependencies stay explained; unsupported controls
 * stay mounted but leave layout, focus, and the accessibility tree. */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test } from "vitest";

import { Choice, ChoiceGrid, SettingReveal, SettingRow } from "./controls";

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
});

function render(node: React.ReactNode) {
  act(() => {
    root.render(node);
  });
  const row = container.querySelector<HTMLElement>(".settings-row");
  if (row === null) throw new Error("no row rendered");
  return row;
}

test("an active row explains what the setting does", () => {
  const row = render(
    <SettingRow label="Edge margin" description="How much goes to shortcuts.">
      <input />
    </SettingRow>,
  );
  expect(row.querySelector(".settings-help")?.textContent).toBe(
    "How much goes to shortcuts.",
  );
  expect(row.dataset.disabled).toBeUndefined();
});

test("a switched-off row shows the reason INSTEAD of the description", () => {
  const row = render(
    <SettingRow
      label="Edge margin"
      description="How much goes to shortcuts."
      disabledReason="Only the Split variant has two zones to divide."
    >
      <input />
    </SettingRow>,
  );
  // One help line, and it is the reason: a reader must not have to pick the
  // relevant sentence out of two.
  const help = [...row.querySelectorAll(".settings-help")].map(
    (element) => element.textContent,
  );
  expect(help).toEqual(["Only the Split variant has two zones to divide."]);
  // The marker CSS dims the label and the control off, and lifts the reason.
  expect(row.dataset.disabled).toBe("");
});

test("a switch shares the label's line, other controls get their own", () => {
  const row = render(
    <SettingRow
      label="Hover magnify"
      description="Enlarges tiles."
      control={<input type="checkbox" className="sb-toggle" />}
    />,
  );
  expect(row.querySelector(".settings-row > .settings-row-suffix")).not.toBe(
    null,
  );
  // No main control, so no empty cell takes up the right column.
  expect(row.querySelector(".settings-row-control")).toBe(null);
});

test("a wide row marks itself so the control gets the whole width", () => {
  const row = render(
    <SettingRow label="Installed" wide>
      <ul />
    </SettingRow>,
  );
  expect(row.dataset.wide).toBe("");
  expect(row.querySelector(".settings-row-control > ul")).not.toBe(null);
});

test("choice tiles expose button state without promising radio-key behavior", () => {
  act(() => {
    root.render(
      <ChoiceGrid label="Position">
        <Choice label="Top" active={true} onClick={() => undefined} />
        <Choice label="Bottom" active={false} onClick={() => undefined} />
      </ChoiceGrid>,
    );
  });
  const group = container.querySelector('[role="group"]');
  expect(group?.getAttribute("aria-label")).toBe("Position");
  expect(
    [...container.querySelectorAll("button")].map((button) =>
      button.getAttribute("aria-pressed"),
    ),
  ).toEqual(["true", "false"]);
  expect(container.querySelector('[role="radio"]')).toBeNull();
});

test("an unavailable setting stays mounted but leaves interaction and accessibility", () => {
  act(() => {
    root.render(
      <SettingReveal visible={false}>
        <SettingRow label="Primary zone">
          <input aria-label="Primary zone" />
        </SettingRow>
      </SettingReveal>,
    );
  });

  const reveal = container.querySelector<HTMLElement>(".settings-reveal");
  const input = container.querySelector("input");
  expect(reveal?.getAttribute("aria-hidden")).toBe("true");
  expect(reveal?.hasAttribute("inert")).toBe(true);
  expect(reveal?.classList.contains("sb-active")).toBe(false);

  act(() => {
    root.render(
      <SettingReveal visible={true}>
        <SettingRow label="Primary zone">
          <input aria-label="Primary zone" />
        </SettingRow>
      </SettingReveal>,
    );
  });
  expect(container.querySelector("input")).toBe(input);
  expect(reveal?.hasAttribute("aria-hidden")).toBe(false);
  expect(reveal?.hasAttribute("inert")).toBe(false);
  expect(reveal?.classList.contains("sb-active")).toBe(true);
});
