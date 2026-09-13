// @vitest-environment happy-dom
import { act, Profiler } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar, type FlyoutRect } from "../store/bar";
import { PluginContent } from "./PluginContent";

const triggerRect: FlyoutRect = { left: 10, top: 10, width: 40, height: 40 };

function tile(pluginId: string) {
  return function TestPluginContent() {
    return (
      <PluginContent
        definition={{
          id: `plugin:${pluginId}:status`,
          pluginId,
          tile: { id: "status", name: "Status", hasFlyout: true },
          meta: { name: "Status" },
        }}
      />
    );
  };
}

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  useSmabar.setState(useSmabar.getInitialState(), true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = false;
});

test("opening another plugin flyout does not render an unrelated tile", () => {
  const Active = tile("active");
  const Unrelated = tile("unrelated");
  const unrelatedRender = vi.fn();
  act(() => {
    root.render(
      <>
        <Active />
        <Profiler id="unrelated" onRender={unrelatedRender}>
          <Unrelated />
        </Profiler>
      </>,
    );
  });
  unrelatedRender.mockClear();

  act(() => {
    useSmabar.getState().toggleFlyout("plugin:active:status", triggerRect);
  });
  expect(unrelatedRender).not.toHaveBeenCalled();

  act(() => {
    useSmabar.getState().closeFlyout();
  });
  expect(unrelatedRender).not.toHaveBeenCalled();
});

test("pushed hover html re-renders the tile only when it appears or leaves", () => {
  const Active = tile("active");
  const activeRender = vi.fn();
  act(() => {
    root.render(
      <Profiler id="active" onRender={activeRender}>
        <Active />
      </Profiler>,
    );
  });
  activeRender.mockClear();

  act(() => {
    useSmabar.getState().setPluginUi("active/status/hover", "<b>1</b>");
  });
  expect(activeRender).toHaveBeenCalled();
  activeRender.mockClear();

  // Two thirds of the bar's live events are hover updates it never shows.
  act(() => {
    useSmabar.getState().setPluginUi("active/status/hover", "<b>2</b>");
  });
  act(() => {
    useSmabar.getState().setPluginUi("active/status/hover", "<b>3</b>");
  });
  expect(activeRender).not.toHaveBeenCalled();

  act(() => {
    useSmabar.getState().setPluginUi("active/status/tile", "<b>x</b>");
  });
  expect(activeRender).toHaveBeenCalled();
});
