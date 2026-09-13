// @vitest-environment happy-dom
import { act, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "./store/bar";
import { App } from "./App";

const { applyTokens, finalizeOverlayClear, renderBar, renderOverlay } =
  vi.hoisted(() => ({
    applyTokens: vi.fn(),
    finalizeOverlayClear: vi.fn(() => Promise.resolve()),
    renderBar: vi.fn(() => null),
    renderOverlay: vi.fn<(props: { onReady?: () => void }) => ReactNode>(
      () => null,
    ),
  }));

vi.mock("./components/bar/BarShell", () => ({ BarShell: renderBar }));
vi.mock("./components/DesktopBackground", () => ({
  DesktopBackground: () => null,
}));
vi.mock("./components/DevFixture", () => ({ DevFixture: () => null }));
vi.mock("./components/overlay/ContextMenuLayer", () => ({
  ContextMenuLayer: () => null,
}));
vi.mock("./components/overlay/Tooltip", () => ({
  TooltipLayer: () => null,
}));
vi.mock("./components/PluginPopup", () => ({ PluginPopup: () => null }));
vi.mock("./components/settings/SettingsPanel", () => ({
  SettingsPanel: () => <div data-settings />,
}));
vi.mock("./components/Toast", () => ({ Toast: () => null }));
vi.mock("./components/NotificationSurface", () => ({
  NotificationSurface: () => null,
}));
vi.mock("./components/OverlaySurface", () => ({
  OverlaySurface: renderOverlay,
}));
vi.mock("./components/OverlayTooltip", () => ({
  OverlayTooltip: () => null,
}));
vi.mock("./ipc/overlay", () => ({ finalizeOverlayClear }));
vi.mock("./theme/apply", () => ({ applyTokenOverrides: applyTokens }));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  applyTokens.mockClear();
  finalizeOverlayClear.mockClear();
  renderBar.mockClear();
  renderOverlay.mockReset();
  renderOverlay.mockImplementation(() => <div data-overlay />);
  useSmabar.setState(useSmabar.getInitialState(), true);
  container = document.createElement("div");
  container.id = "root";
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
});

test("token overrides update the root without re-rendering the app", () => {
  act(() => {
    root.render(<App />);
  });
  expect(renderBar).toHaveBeenCalledOnce();
  expect(applyTokens).toHaveBeenCalledOnce();

  act(() => {
    const store = useSmabar.getState();
    store.setAppearance({
      ...store.appearance,
      tokens: { "--sb-scale": "1.25" },
    });
  });

  expect(applyTokens).toHaveBeenCalledTimes(2);
  expect(applyTokens).toHaveBeenLastCalledWith({ "--sb-scale": "1.25" });
  expect(renderBar).toHaveBeenCalledOnce();
});

test("reports when the overlay DOM has been cleared", async () => {
  act(() => {
    root.render(<App role="overlay" />);
  });
  expect(container.querySelector("[data-overlay]")).not.toBeNull();

  renderOverlay.mockImplementation(() => null);
  await act(async () => {
    root.render(<App role="overlay" />);
    await Promise.resolve();
  });

  expect(finalizeOverlayClear).toHaveBeenCalledOnce();
});

test("settings signal readiness only after their lazy content mounts", async () => {
  const mounted = vi.fn(() => {
    expect(container.querySelector("[data-settings]")).not.toBeNull();
  });
  await act(async () => {
    root.render(<App role="settings" onMounted={mounted} />);
    await Promise.resolve();
  });
  await vi.waitFor(() => {
    expect(mounted).toHaveBeenCalledOnce();
  });
  expect(renderBar).not.toHaveBeenCalled();
});

test("overlay readiness waits for its listeners and locale keeps the same surface", () => {
  const mounted = vi.fn();
  act(() => {
    root.render(<App role="overlay" onMounted={mounted} />);
  });
  const surface = container.querySelector("[data-overlay]");
  expect(surface).not.toBeNull();
  expect(mounted).not.toHaveBeenCalled();
  renderOverlay.mock.calls[0]?.[0].onReady?.();
  expect(mounted).toHaveBeenCalledOnce();
  act(() => {
    useSmabar.getState().bumpLocaleVersion();
  });
  expect(container.querySelector("[data-overlay]")).toBe(surface);
  expect(renderOverlay).toHaveBeenCalledTimes(2);
  expect(mounted).toHaveBeenCalledOnce();
});
