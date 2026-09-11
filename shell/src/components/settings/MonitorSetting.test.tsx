// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../../store/bar";
import { MonitorSetting } from "./MonitorSetting";

const { callMock, getMonitorStateMock } = vi.hoisted(() => ({
  callMock: vi.fn(() => Promise.resolve()),
  getMonitorStateMock: vi.fn(),
}));
vi.mock("../../ipc/call", () => ({ call: callMock }));
vi.mock("../../ipc/monitors", () => ({
  getMonitorState: getMonitorStateMock,
  onMonitorsChanged: vi.fn(() => Promise.resolve(() => undefined)),
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  callMock.mockClear();
  getMonitorStateMock.mockResolvedValue({
    monitors: [
      {
        id: "right",
        label: "HDMI-A-0",
        width: 2560,
        height: 1440,
        scaleFactor: 1,
        primary: true,
      },
    ],
    effectiveId: "right",
    preferredConnected: false,
  });
  useSmabar.setState(useSmabar.getInitialState(), true);
  useSmabar.getState().setLayout({
    ...useSmabar.getState().layout,
    monitor: { id: "gone", label: "Laptop" },
  });
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

test("selects connected, automatic, and disconnected monitor preferences", async () => {
  act(() => {
    root.render(<MonitorSetting />);
  });
  await act(() => Promise.resolve());

  const select = container.querySelector("select");
  if (select === null) throw new Error("monitor select is missing");
  expect([...select.options].map((option) => option.textContent)).toEqual([
    "Automatic (system primary monitor)",
    "HDMI-A-0 — 2560×1440 (primary)",
    "Laptop (disconnected)",
  ]);
  expect(container.querySelector('[role="status"]')).not.toBeNull();

  act(() => {
    select.value = "right";
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(useSmabar.getState().layout.monitor).toEqual({
    id: "right",
    label: "HDMI-A-0",
  });
  expect(callMock).toHaveBeenLastCalledWith("update_config", {
    path: "layout.monitor",
    value: { id: "right", label: "HDMI-A-0" },
  });

  act(() => {
    select.value = "";
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(useSmabar.getState().layout.monitor).toBeNull();
});
