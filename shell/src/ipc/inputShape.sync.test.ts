// @vitest-environment happy-dom
import { invoke } from "@tauri-apps/api/core";
import { expect, test, vi } from "vitest";

import { useSmabar } from "../store/bar";
import { initInputShape, requestBarGeometry } from "./inputShape";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

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
});
