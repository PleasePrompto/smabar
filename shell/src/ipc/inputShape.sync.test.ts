// @vitest-environment happy-dom
import { invoke } from "@tauri-apps/api/core";
import { afterEach, expect, test, vi } from "vitest";

import { useSmabar } from "../store/bar";
import { initInputShape, requestBarGeometry } from "./inputShape";
import { reportError } from "./log";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));
vi.mock("./log", () => ({ reportError: vi.fn() }));

afterEach(() => vi.useRealTimers());

test("hidden surfaces report edge changes and input regions without animation frames", async () => {
  vi.spyOn(window, "requestAnimationFrame").mockReturnValue(1);
  const dock = document.createElement("div");
  dock.setAttribute("data-bar-dock", "");
  dock.setAttribute("data-input-region", "");
  let top = 680;
  dock.getBoundingClientRect = () => new DOMRect(200, top, 700, 60);
  document.body.append(dock);
  useSmabar.setState({
    layout: { ...useSmabar.getState().layout, position: "bottom" },
  });
  initInputShape();
  await Promise.resolve();
  expect(invoke).toHaveBeenCalledWith(
    "set_bar_geometry",
    expect.objectContaining({ position: "bottom" }),
  );

  vi.mocked(invoke).mockClear();
  top = 20;
  useSmabar.setState({
    layout: { ...useSmabar.getState().layout, position: "top" },
  });
  requestBarGeometry();
  await Promise.resolve();
  expect(invoke).toHaveBeenCalledWith(
    "set_bar_geometry",
    expect.objectContaining({
      position: "top",
      rect: { x: 200, y: 20, w: 700, h: 60 },
    }),
  );
  expect(invoke).toHaveBeenCalledWith("set_input_shape", {
    rects: [{ x: 199, y: 19, w: 702, h: 62 }],
  });

  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  vi.mocked(invoke)
    .mockClear()
    .mockRejectedValueOnce(new Error("input not ready"))
    .mockRejectedValueOnce(new Error("geometry not ready"));
  top = 30;
  requestBarGeometry();
  await Promise.resolve();
  await Promise.resolve();
  expect(invoke).toHaveBeenCalledTimes(2);
  expect(reportError).toHaveBeenCalledTimes(2);
  expect(vi.getTimerCount()).toBe(1);
  // No DOM/store activity: both commands recover through one bounded retry.
  await vi.advanceTimersByTimeAsync(1_000);
  expect(invoke).toHaveBeenCalledTimes(4);
  expect(invoke).toHaveBeenCalledWith("set_input_shape", {
    rects: [{ x: 199, y: 29, w: 702, h: 62 }],
  });
  await vi.advanceTimersByTimeAsync(1_000);
  expect(invoke).toHaveBeenCalledTimes(4);
  expect(vi.getTimerCount()).toBe(0);
});
