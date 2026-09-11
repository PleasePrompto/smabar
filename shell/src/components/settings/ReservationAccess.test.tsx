// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, test, vi } from "vitest";

import { ReservationAccess } from "./ReservationAccess";

const { callMock, reportMock } = vi.hoisted(() => ({
  callMock: vi.fn(),
  reportMock: vi.fn(),
}));
vi.mock("../../ipc/call", () => ({ call: callMock }));
vi.mock("../../ipc/log", () => ({ reportError: reportMock }));

test("permission stays opt-in, retries failures and disappears after access is granted", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  let granted = false;
  let fail = true;
  callMock.mockImplementation((command: string) => {
    if (command === "request_reservation_access" && fail)
      return Promise.reject(new Error("IPC unavailable"));
    return Promise.resolve(granted ? "ready" : "permissionRequired");
  });
  try {
    await act(async () => {
      root.render(<ReservationAccess />);
      await Promise.resolve();
    });
    expect(callMock).toHaveBeenCalledWith("get_reservation_status");
    expect(callMock).not.toHaveBeenCalledWith("request_reservation_access");
    const button = host.querySelector("button");
    expect(button).not.toBeNull();
    await act(async () => {
      button?.click();
      await Promise.resolve();
    });
    expect(host.querySelector('[role="alert"]')).not.toBeNull();
    expect(reportMock).toHaveBeenCalled();
    fail = false;
    await act(async () => {
      button?.click();
      await Promise.resolve();
    });
    expect(host.querySelector('[role="alert"]')).toBeNull();
    expect(host.querySelector("button")).not.toBeNull();
    granted = true;
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      await Promise.resolve();
    });
    expect(host.textContent).toBe("");
  } finally {
    act(() => {
      root.unmount();
    });
    host.remove();
  }
});
