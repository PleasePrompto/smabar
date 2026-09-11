// @vitest-environment happy-dom
import { beforeEach, expect, test } from "vitest";

import { useSmabar } from "../store/bar";
import { collectBarGeometry, collectRects } from "./inputShape";

const trigger = { left: 20, top: 30, width: 40, height: 50 };

beforeEach(() => {
  document.body.replaceChildren();
  useSmabar.setState(useSmabar.getInitialState(), true);
});

test("flyouts never expand the bar input region", () => {
  const surface = document.createElement("div");
  surface.setAttribute("data-input-region", "");
  surface.getBoundingClientRect = () => new DOMRect(10, 20, 100, 80);
  document.body.appendChild(surface);

  useSmabar.getState().peekFlyout("clock", trigger);
  expect(collectRects()).toEqual([{ x: 9, y: 19, w: 102, h: 82 }]);

  useSmabar.getState().toggleFlyout("clock", trigger);
  expect(collectRects()).toEqual([{ x: 9, y: 19, w: 102, h: 82 }]);
});

test("autohide uses stable surface and hotzone rects instead of transformed rows", () => {
  const row = document.createElement("div");
  row.setAttribute("data-bar-root", "");
  row.setAttribute("data-input-region", "");
  row.getBoundingClientRect = () => new DOMRect(0, 760, 1_200, 40);

  const surface = document.createElement("div");
  surface.setAttribute("data-input-region", "");
  surface.getBoundingClientRect = () => new DOMRect(200, 760, 800, 40);

  const hotzone = document.createElement("div");
  hotzone.setAttribute("data-input-region", "");
  hotzone.getBoundingClientRect = () => new DOMRect(0, 796, 1_200, 4);
  document.body.append(row, surface, hotzone);

  const state = useSmabar.getState();
  state.setLayout({ ...state.layout, behavior: "autohide" });
  expect(collectRects()).toEqual([
    { x: 199, y: 759, w: 802, h: 42 },
    { x: -1, y: 795, w: 1_202, h: 6 },
  ]);
});

test("bar geometry includes a stable content-sized native surface", () => {
  const dock = document.createElement("div");
  dock.setAttribute("data-bar-dock", "");
  dock.getBoundingClientRect = () => new DOMRect(200.2, 684.4, 700.1, 79.3);
  document.body.append(dock);
  const state = useSmabar.getState();
  state.setLayout({ ...state.layout, width: "auto" });

  expect(collectBarGeometry()).toEqual({
    position: "bottom",
    behavior: "reserve",
    rect: { x: 200, y: 684, w: 701, h: 80 },
    surface: { width: 701, height: 84 },
  });

  state.setLayout({ ...useSmabar.getState().layout, behavior: "float" });
  expect(collectBarGeometry()).toEqual({
    position: "bottom",
    behavior: "float",
    rect: { x: 200, y: 684, w: 701, h: 80 },
    surface: { width: 701, height: 84 },
  });
});

test("a hidden solo row keeps the surface tall but leaves reservation and shape", () => {
  document.body.innerHTML = "";
  const dock = document.createElement("div");
  dock.setAttribute("data-bar-dock", "");
  dock.setAttribute("data-input-region", "");
  const hidden = document.createElement("div");
  hidden.setAttribute("data-bar-root", "");
  hidden.setAttribute("data-bar-hidden", "");
  const shown = document.createElement("div");
  shown.setAttribute("data-bar-root", "");
  shown.setAttribute("data-input-region", "");
  dock.append(hidden, shown);
  document.body.append(dock);
  dock.getBoundingClientRect = () => new DOMRect(200, 600, 700, 160);
  hidden.getBoundingClientRect = () => new DOMRect(200, 600, 700, 76);
  shown.getBoundingClientRect = () => new DOMRect(200, 680, 700, 80);
  useSmabar.setState({
    layout: {
      ...useSmabar.getState().layout,
      position: "bottom",
      behavior: "reserve",
      width: "auto",
    },
  });
  const geometry = collectBarGeometry();
  expect(geometry?.rect).toEqual({ x: 200, y: 680, w: 700, h: 80 });
  expect(geometry?.surface.height).toBe(window.innerHeight - 600);
  // The dock's input region shrinks to the visible row as well.
  expect(collectRects()).toEqual([
    { x: 199, y: 679, w: 702, h: 82 },
    { x: 199, y: 679, w: 702, h: 82 },
  ]);
});
