// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, test, vi } from "vitest";

import { ResizeEdges } from "./ResizeEdges";

const { resize, report } = vi.hoisted(() => ({
  resize: vi.fn(() => Promise.resolve()),
  report: vi.fn(),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ startResizeDragging: resize }),
}));
vi.mock("../../ipc/log", () => ({ reportError: report }));

test("every settings edge uses native resize and reports rejected gestures", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    act(() => {
      root.render(<ResizeEdges />);
    });
    const handles = [
      ...container.querySelectorAll<HTMLElement>(".settings-resize-edge"),
    ];
    expect(handles.map((handle) => handle.dataset.direction)).toEqual([
      "North",
      "South",
      "East",
      "West",
      "NorthEast",
      "NorthWest",
      "SouthEast",
      "SouthWest",
    ]);
    for (const handle of handles) {
      handle.dispatchEvent(
        new MouseEvent("mousedown", { bubbles: true, button: 2 }),
      );
    }
    expect(resize).not.toHaveBeenCalled();
    for (const handle of handles) {
      handle.dispatchEvent(
        new MouseEvent("mousedown", { bubbles: true, button: 0 }),
      );
    }
    expect(resize.mock.calls).toEqual(
      handles.map((handle) => [handle.dataset.direction]),
    );
    const error = new Error("The compositor rejected the resize gesture");
    resize.mockRejectedValueOnce(error);
    await act(async () => {
      handles[0]?.dispatchEvent(
        new MouseEvent("mousedown", { bubbles: true, button: 0 }),
      );
      await Promise.resolve();
    });
    expect(report).toHaveBeenCalledWith(error);
  } finally {
    act(() => {
      root.unmount();
    });
    container.remove();
  }
});
