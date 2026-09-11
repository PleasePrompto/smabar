// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test } from "vitest";

import { useSmabar } from "../store/bar";
import { PluginContent } from "./PluginContent";

let host: HTMLDivElement;
let root: Root | null;

beforeEach(() => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  useSmabar.setState(useSmabar.getInitialState(), true);
  root = null;
  host = document.createElement("div");
  document.body.appendChild(host);
});

afterEach(() => {
  if (root !== null) {
    act(() => {
      root?.unmount();
    });
  }
  host.remove();
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = false;
});

test.each([false, true])(
  "a custom SVG takes precedence with usePluginIcon=%s",
  (usePluginIcon) => {
    useSmabar.getState().setPluginUi("brand/status/tile", "");
    const definition = {
      id: "plugin:brand:status",
      pluginId: "brand",
      iconDataUrl: "data:image/png;base64,aWNvbg==",
      tile: {
        id: "status",
        name: "Status",
        usePluginIcon,
        iconSvg: '<svg viewBox="0 0 24 24"><path d="M2 2h20v20H2z"/></svg>',
      },
      meta: { name: "Status" },
    };
    root = createRoot(host);

    act(() => {
      root?.render(<PluginContent definition={definition} />);
    });

    const iconHost = host.querySelector<HTMLElement>(".plugin-icon");
    expect(iconHost).not.toBeNull();
    expect(iconHost?.shadowRoot?.querySelector("svg") ?? null).not.toBeNull();
    expect(iconHost?.shadowRoot?.querySelector("img") ?? null).toBeNull();
    expect(host.querySelector("button")?.children).toHaveLength(1);
  },
);

test("dedicated hover content marks the tile as tooltip-free", () => {
  useSmabar.getState().setPluginUi("status/main/tile", "<span>Ready</span>");
  useSmabar.getState().setPluginUi("status/main/hover", "<span>Details</span>");
  root = createRoot(host);

  act(() => {
    root?.render(
      <PluginContent
        definition={{
          id: "plugin:status:main",
          pluginId: "status",
          tile: { id: "main", name: "Status", hasFlyout: true },
          meta: { name: "Status" },
        }}
      />,
    );
  });

  expect(host.querySelector("[data-hover-flyout]")).not.toBeNull();
});

test.each([
  { choice: "omitted", usePluginIcon: undefined, hasFile: true, shown: false },
  { choice: "false", usePluginIcon: false, hasFile: true, shown: false },
  { choice: "true", usePluginIcon: true, hasFile: true, shown: true },
  {
    choice: "true without a file",
    usePluginIcon: true,
    hasFile: false,
    shown: false,
  },
])(
  "the folder icon in a cover is optional: $choice",
  ({ usePluginIcon, hasFile, shown }) => {
    useSmabar.getState().setPluginUi("brand/status/tile", "");
    root = createRoot(host);
    act(() => {
      root?.render(
        <PluginContent
          definition={{
            id: "plugin:brand:status",
            pluginId: "brand",
            iconDataUrl: hasFile ? "data:image/png;base64,aWNvbg==" : undefined,
            tile: { id: "status", name: "Status", usePluginIcon },
            meta: { name: "Status" },
          }}
        />,
      );
    });
    const iconHost = host.querySelector(".plugin-icon");
    if (shown) {
      expect(iconHost?.shadowRoot?.querySelector("img")?.src).toBe(
        "data:image/png;base64,aWNvbg==",
      );
    } else {
      expect(iconHost).toBeNull();
    }
  },
);
