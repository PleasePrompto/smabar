// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { NotificationSurface } from "./NotificationSurface";
import { useSmabar } from "../store/bar";
import type { UpdateInfo } from "../store/types";

const { measure, close, install, open } = vi.hoisted(() => ({
  measure: vi.fn(() => Promise.resolve()),
  close: vi.fn(() => Promise.resolve()),
  install: vi.fn(() => Promise.resolve()),
  open: vi.fn(() => Promise.resolve()),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));
vi.mock("../ipc/surface", () => ({
  reportNotificationMeasure: measure,
  closeCurrentSurface: close,
  stageNotificationUpdate: () => Promise.resolve(),
  openSettings: open,
}));
vi.mock("../ipc/update", () => ({
  installUpdate: install,
  checkUpdate: () => Promise.resolve(),
}));

const offer: UpdateInfo = {
  version: "1.1.0",
  installer: "system",
  notes: null,
  date: null,
};
let root: Root;
let container: HTMLDivElement;
beforeEach(() => {
  vi.clearAllMocks();
  useSmabar.setState(useSmabar.getInitialState(), true);
  useSmabar.setState({
    updateChannel: "app",
    updateOffer: offer,
    updateStatus: { state: "available", ...offer },
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
  vi.useRealTimers();
});

test("the notification stays visible without plugin popups, installs the displayed version and has an accessible dismiss action", async () => {
  vi.useFakeTimers();
  useSmabar.setState({
    popups: { ...useSmabar.getState().popups, enabled: false },
  });
  act(() => {
    root.render(<NotificationSurface />);
  });
  expect(container.textContent).toContain("smabar 1.1.0 is available");
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(container.querySelector(".app-update-notification")).not.toBeNull();
  await act(async () => {
    container.querySelector<HTMLButtonElement>(".sb-btn-primary")?.click();
    await Promise.resolve();
  });
  expect(install).toHaveBeenCalledWith("1.1.0");
  expect(open).toHaveBeenCalledWith("system/updates");
  await act(async () => {
    container
      .querySelector<HTMLButtonElement>('[aria-label="Dismiss notification"]')
      ?.click();
    await Promise.resolve();
  });
  expect(container.querySelector(".app-update-notification")).toBeNull();
  expect(useSmabar.getState().updateOffer).toEqual(offer);
  expect(close).toHaveBeenCalledOnce();
});

test("same-size status changes remeasure after native staging and dismissal preserves other notices", async () => {
  act(() => {
    root.render(<NotificationSurface />);
  });
  measure.mockClear();
  act(() => {
    useSmabar.setState({ updateStatus: { state: "checking" } });
  });
  expect(measure).toHaveBeenCalled();
  act(() => {
    useSmabar.getState().setNotice("settings.themes.saved");
  });
  await act(async () => {
    container
      .querySelector<HTMLButtonElement>('[aria-label="Dismiss notification"]')
      ?.click();
    await Promise.resolve();
  });
  expect(container.querySelector("[data-shell-toast]")).not.toBeNull();
  expect(close).not.toHaveBeenCalled();
});

test("unsupported installers offer details, and the first-start gate suppresses the update", async () => {
  useSmabar.setState({
    updateOffer: { ...offer, installer: null },
    legalRequired: true,
  });
  act(() => {
    root.render(<NotificationSurface />);
  });
  expect(container.querySelector(".app-update-notification")).toBeNull();
  act(() => {
    useSmabar.setState({ legalRequired: false });
  });
  expect(container.querySelector(".sb-btn-primary")?.textContent).toContain(
    "Details",
  );
  expect(container.querySelector(".app-update-notification p")).toBeNull();
  await act(async () => {
    container.querySelector<HTMLButtonElement>(".sb-btn-primary")?.click();
    await Promise.resolve();
  });
  expect(open).toHaveBeenCalledWith("system/updates");
  expect(install).not.toHaveBeenCalled();
});

test("download hides without destroying the listener, then failure restores the notification and handoff closes it", () => {
  act(() => {
    root.render(<NotificationSurface />);
  });
  act(() => {
    useSmabar.setState({
      updateStatus: {
        state: "downloading",
        version: offer.version,
        received: 0,
        total: null,
      },
    });
  });
  expect(container.querySelector(".app-update-notification")).toBeNull();
  expect(measure).toHaveBeenLastCalledWith(
    expect.objectContaining({ popup: null, notice: null }),
  );
  expect(close).not.toHaveBeenCalled();
  act(() => {
    useSmabar.setState({
      updateStatus: { state: "failed", phase: "install", message: "Offline" },
    });
  });
  expect(
    container.querySelector(".app-update-notification")?.textContent,
  ).toContain("The update could not be installed");
  act(() => {
    useSmabar.setState({
      updateStatus: { state: "installing", version: offer.version },
    });
  });
  expect(close).not.toHaveBeenCalled();
  act(() => {
    useSmabar.setState({
      updateStatus: {
        state: "handedOff",
        version: offer.version,
        path: "/tmp/update.deb",
        opened: true,
      },
    });
  });
  expect(container.querySelector(".app-update-notification")).toBeNull();
  expect(close).toHaveBeenCalledOnce();
});
