// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../store/bar";
import { PluginContent } from "./PluginContent";

const { readableTextOn } = vi.hoisted(() => ({
  readableTextOn: vi.fn(() => "#ffffff"),
}));

vi.mock("../theme/contrast", () => ({ readableTextOn }));

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  useSmabar.setState(useSmabar.getInitialState(), true);
  readableTextOn.mockClear();
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

test("a tile's accent is resolved once, not on every pushed update", () => {
  const definition = {
    id: "plugin:brand:status",
    pluginId: "brand",
    tile: { id: "status", name: "Status", accent: "#336699" },
    meta: { name: "Status" },
  };
  act(() => {
    root.render(<PluginContent definition={definition} />);
  });
  expect(readableTextOn).toHaveBeenCalledTimes(1);

  act(() => {
    useSmabar.getState().setPluginUi("brand/status/tile", "<b>1</b>");
  });
  act(() => {
    useSmabar.getState().setPluginUi("brand/status/tile", "<b>2</b>");
  });
  expect(readableTextOn).toHaveBeenCalledTimes(1);
  expect(host.querySelector('[style*="--sb-accent"]')).not.toBeNull();
});
