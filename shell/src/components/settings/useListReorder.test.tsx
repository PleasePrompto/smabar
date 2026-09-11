// @vitest-environment happy-dom
/**
 * The first DOM-level drag test in this repo.
 *
 * happy-dom lays nothing out — every `getBoundingClientRect` is a zero-sized
 * rect — so the rows are given their geometry explicitly. That makes the
 * per-frame pixel maths untestable here (it would only assert the stubs), and
 * those stay covered purely in `dragReorder.test.ts`. What IS worth asserting
 * is everything around them: which gestures arm a drag, which must not, what
 * reaches `onReorder`, and — most of all — that a torn-down drag leaves no
 * trace. A stuck `reordering` flag makes `inputShape.ts` keep a full-window
 * input rect forever, and the whole desktop behind the bar stops taking
 * clicks.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../../store/bar";

import { useListReorder } from "./useListReorder";

const ROW_HEIGHT = 34;
const GAP = 4;

let container: HTMLDivElement;
let root: Root;
const reorders: [number, number][] = [];

/** A list of `count` reorderable rows, each with a handle and a button. */
function List({ count }: { count: number }) {
  const ref = { current: null } as React.RefObject<HTMLDivElement | null>;
  return <Inner count={count} listRef={ref} />;
}

function Inner({
  count,
  listRef,
}: {
  count: number;
  listRef: React.RefObject<HTMLDivElement | null>;
}) {
  useListReorder(listRef, {
    count,
    onReorder: (from, to) => reorders.push([from, to]),
  });
  return (
    <div className="sb-list" ref={listRef}>
      {Array.from({ length: count }, (_, index) => (
        <div key={index} className="sb-row" data-reorder-index={index}>
          <span data-drag-handle>grip</span>
          <button type="button">hide</button>
        </div>
      ))}
      <div className="sb-row">off</div>
    </div>
  );
}

/** Gives the list and its rows the geometry happy-dom will not compute. */
function layout(): void {
  const list = container.querySelector<HTMLElement>(".sb-list");
  if (list === null) throw new Error("no list");
  const rows = [...list.querySelectorAll<HTMLElement>("[data-reorder-index]")];
  list.getBoundingClientRect = () =>
    new DOMRect(0, 0, 300, rows.length * (ROW_HEIGHT + GAP));
  rows.forEach((row, index) => {
    const top = index * (ROW_HEIGHT + GAP);
    row.getBoundingClientRect = () => new DOMRect(0, top, 300, ROW_HEIGHT);
  });
}

function pointer(type: string, target: Element, y: number, extra = {}): void {
  act(() => {
    target.dispatchEvent(
      new PointerEvent(type, {
        pointerId: 1,
        isPrimary: true,
        button: 0,
        buttons: 1,
        clientX: 10,
        clientY: y,
        bubbles: true,
        composed: true,
        ...extra,
      }),
    );
  });
}

const list = () => {
  const node = container.querySelector<HTMLElement>(".sb-list");
  if (node === null) throw new Error("no list");
  return node;
};
const handleOf = (index: number): HTMLElement => {
  const handles = [
    ...container.querySelectorAll<HTMLElement>("[data-drag-handle]"),
  ];
  const handle = handles[index];
  if (handle === undefined) throw new Error(`no handle ${String(index)}`);
  return handle;
};
const marker = () => document.querySelector(".settings-drop-marker");
/**
 * The shift a row currently carries, along Y.
 *
 * Read through `getPropertyValue`, not `style.translate`: happy-dom has no
 * accessor for the standalone property, but `setProperty` stores an unknown
 * name verbatim, so this round-trips while the accessor is undefined.
 */
const shiftOf = (row: Element): number =>
  Number.parseFloat(
    (row as HTMLElement).style.getPropertyValue("translate").split(" ")[1] ??
      "0",
  ) || 0;
const rowsOf = () => [...list().querySelectorAll<HTMLElement>(".sb-row")];
const nothingShifted = () => rowsOf().every((row) => shiftOf(row) === 0);

beforeEach(() => {
  reorders.length = 0;
  // Synchronous frames: the hook batches its updates into rAF.
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    cb(0);
    return 1;
  });
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  act(() => {
    root.render(<List count={3} />);
  });
  layout();
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  useSmabar.getState().setReordering(false);
  vi.unstubAllGlobals();
});

test("a handle plus movement past the threshold arms a drag", () => {
  pointer("pointerdown", handleOf(0), 10);
  // Under the 6px tolerance: still just a press.
  pointer("pointermove", list(), 14);
  expect(list().hasAttribute("data-reordering")).toBe(false);

  pointer("pointermove", list(), 30);
  expect(list().hasAttribute("data-reordering")).toBe(true);
  expect(useSmabar.getState().reordering).toBe(true);
  // Portalled to <body>: .settings-panel is transformed in its default
  // placement, which would make it the containing block of a fixed child,
  // and .settings-group-content would clip it at the scroll edge.
  expect(marker()?.parentElement).toBe(document.body);
});

test("a press on a button in the row arms nothing", () => {
  const button = container.querySelector("button");
  if (button === null) throw new Error("no button");
  pointer("pointerdown", button, 10);
  pointer("pointermove", list(), 60);
  expect(list().hasAttribute("data-reordering")).toBe(false);
  expect(marker()).toBeNull();
  expect(nothingShifted()).toBe(true);
});

test("a row without a handle arms nothing", () => {
  const rows = [...container.querySelectorAll(".sb-row")];
  const tail = rows[rows.length - 1];
  if (tail === undefined) throw new Error("no tail row");
  pointer("pointerdown", tail, 10);
  pointer("pointermove", list(), 60);
  expect(list().hasAttribute("data-reordering")).toBe(false);
});

test("dropping past the next row's center commits the move", () => {
  pointer("pointerdown", handleOf(0), 10);
  // Row 1 spans 38…72, so its center is 55; land the dragged row past it.
  pointer("pointermove", list(), 70);
  pointer("pointerup", list(), 70, { buttons: 0 });
  expect(reorders).toEqual([[0, 1]]);
  expect(marker()).toBeNull();
  expect(nothingShifted()).toBe(true);
  expect(useSmabar.getState().reordering).toBe(false);
});

test("dragging the last row above the first one drops it at the very top", () => {
  // The regression this file existed without: the slot used to be read from
  // the CLAMPED position, which puts the dragged row's centre exactly on row
  // 0's centre — and insertionIndex breaks on a strict `<`, so the tie went
  // to the row below and index 0 could not be reached however far you pulled.
  // Before the fix this commits [2, 1].
  pointer("pointerdown", handleOf(2), 80);
  pointer("pointermove", list(), -10);
  pointer("pointerup", list(), -10, { buttons: 0 });
  expect(reorders).toEqual([[2, 0]]);
});

test("the rows the dragged one passes step aside, the fixed tail does not", () => {
  pointer("pointerdown", handleOf(0), 10);
  // Row 0 heading past row 2's centre (93): both rows it overtakes move up by
  // one slot — 34px of row plus the 4px gap.
  pointer("pointermove", list(), 100);
  const rows = rowsOf();
  expect(shiftOf(rows[1] as Element)).toBe(-38);
  expect(shiftOf(rows[2] as Element)).toBe(-38);
  // The dimmed row without data-reorder-index has no place in the order and
  // must not budge.
  expect(shiftOf(rows[3] as Element)).toBe(0);
  // And the dragged row itself is clamped into the hole it opened.
  expect(shiftOf(rows[0] as Element)).toBe(76);
});

test("a drag that never leaves the first slot commits nothing", () => {
  // The unclamped slot now reaches above the list; that must not turn into a
  // write of the order the list already has.
  pointer("pointerdown", handleOf(0), 10);
  pointer("pointermove", list(), -40);
  pointer("pointerup", list(), -40, { buttons: 0 });
  expect(reorders).toEqual([]);
  expect(nothingShifted()).toBe(true);
});

test("escape aborts without committing", () => {
  pointer("pointerdown", handleOf(0), 10);
  pointer("pointermove", list(), 70);
  act(() => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  });
  expect(reorders).toEqual([]);
  expect(marker()).toBeNull();
  expect(nothingShifted()).toBe(true);
  expect(useSmabar.getState().reordering).toBe(false);
});

test("unmounting mid-drag leaves no marker and no stuck flag", () => {
  // The one failure that survives everything: a stuck `reordering` keeps the
  // window's input shape full-screen and the desktop stops taking clicks.
  pointer("pointerdown", handleOf(0), 10);
  pointer("pointermove", list(), 70);
  expect(useSmabar.getState().reordering).toBe(true);

  const rows = rowsOf();
  act(() => {
    root.unmount();
  });
  expect(marker()).toBeNull();
  // A leftover translate would survive into the NEXT drag and every span it
  // measures would be wrong — worse than the stuck flag, because it is silent.
  expect(rows.every((row) => shiftOf(row) === 0)).toBe(true);
  expect(useSmabar.getState().reordering).toBe(false);
  // afterEach unmounts again; that must stay harmless.
  root = createRoot(document.createElement("div"));
});
