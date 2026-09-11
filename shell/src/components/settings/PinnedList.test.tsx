// @vitest-environment happy-dom
/**
 * Renaming a pin opens an input inside the settings panel, and the panel
 * closes on Escape from a `window` listener (SettingsPanel). Aborting a
 * rename must therefore stop the key at the field — without that it threw the
 * whole panel away, losing the place the user was editing.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { call } from "../../ipc/call";
import { useSmabar } from "../../store/bar";

import { PinnedList } from "./PinnedList";

vi.mock("../../ipc/call", () => ({ call: vi.fn(() => Promise.resolve()) }));

/** The entry array of the most recent `shortcuts.pinned` write. */
function lastPinnedWrite(): unknown {
  const calls = vi
    .mocked(call)
    .mock.calls.filter(
      ([command, args]) =>
        command === "update_config" &&
        (args as { path?: string } | undefined)?.path === "shortcuts.pinned",
    );
  const last = calls.at(-1);
  if (last === undefined) throw new Error("no shortcuts.pinned write");
  return (last[1] as { value: unknown }).value;
}

let container: HTMLDivElement;
let root: Root;
let escapesReachingWindow: number;
const onWindowKey = (event: KeyboardEvent) => {
  if (event.key === "Escape") escapesReachingWindow += 1;
};

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  escapesReachingWindow = 0;
  vi.mocked(call).mockClear();
  window.addEventListener("keydown", onWindowKey);

  const store = useSmabar.getState();
  store.setShortcuts({
    ...store.shortcuts,
    pinned: [
      { id: "a", label: "Files", icons: [], separator: false },
      { id: "b", label: "", icons: [], separator: true },
    ],
    entries: [
      { id: "a", desktopId: "files.desktop" },
      { id: "b", separator: true },
    ],
  });
  act(() => {
    root.render(<PinnedList />);
  });
});
afterEach(() => {
  window.removeEventListener("keydown", onWindowKey);
  act(() => {
    root.unmount();
  });
  container.remove();
});

/**
 * Types into a controlled input. Assigning `.value` alone is invisible to
 * React — its value tracker sees no change and the state never updates — so
 * the native setter has to be called before the event is dispatched.
 */
function type(field: HTMLInputElement, value: string) {
  // Reflect.set runs the PROTOTYPE's setter with `field` as the receiver,
  // which is what bypasses the tracker without detaching the method.
  Reflect.set(HTMLInputElement.prototype, "value", value, field);
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

function startRename() {
  const pencil = container.querySelector<HTMLButtonElement>(
    'button[aria-label="Rename"]',
  );
  if (pencil === null) throw new Error("no rename button");
  act(() => {
    pencil.click();
  });
  const field = container.querySelector<HTMLInputElement>(
    'input[aria-label="Rename"]',
  );
  if (field === null) throw new Error("no rename field");
  return field;
}

test("only a renameable pin offers a rename button", () => {
  // The separator has no name of its own, so it must not offer one.
  expect(
    container.querySelectorAll('button[aria-label="Rename"]'),
  ).toHaveLength(1);
});

test("escape closes the editor and does NOT reach the panel", () => {
  const field = startRename();
  act(() => {
    field.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
  });
  expect(container.querySelector('input[aria-label="Rename"]')).toBe(null);
  expect(escapesReachingWindow).toBe(0);
});

test("escape anywhere else still reaches the panel", () => {
  // The stop is the field's alone: closing the panel with Escape has to keep
  // working while nothing is being renamed.
  act(() => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  });
  expect(escapesReachingWindow).toBe(1);
});

test("a name is written to the entry, an emptied field drops the override", () => {
  const field = startRename();
  act(() => {
    type(field, "Dateien");
    field.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
    );
  });
  expect(container.querySelector('input[aria-label="Rename"]')).toBe(null);
  expect(lastPinnedWrite()).toEqual([
    { id: "a", desktopId: "files.desktop", label: "Dateien" },
    { id: "b", separator: true },
  ]);

  // Emptying it means "no name of my own": the key has to GO, not become "",
  // or the core would resolve an empty label instead of the app's own name.
  const again = startRename();
  act(() => {
    type(again, "   ");
    again.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
    );
  });
  expect(lastPinnedWrite()).toEqual([
    { id: "a", desktopId: "files.desktop" },
    { id: "b", separator: true },
  ]);
});
