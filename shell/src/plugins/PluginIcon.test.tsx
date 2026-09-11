// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { PluginIcon } from "./PluginIcon";
import { resetMarkupReports } from "./markupReport";

const logged: { message: string; options?: unknown }[] = [];

vi.mock("../ipc/log", () => ({
  uiLog: (_level: string, message: string, options?: unknown) => {
    logged.push({ message, options });
  },
}));

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  logged.length = 0;
  resetMarkupReports();
  host = document.createElement("div");
  document.body.appendChild(host);
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

test("renders one sanitized custom SVG icon", () => {
  const source = `<svg viewBox="0 0 24 24" style="position:fixed" onclick="evil()">
    <path class="surface-tile" fill="url(https://evil.example/icon.svg)" d="M2 2h20v20H2z"/>
    <circle fill="url(#brand)" cx="12" cy="12" r="3"/>
    <script>evil()</script>
  </svg>`;
  act(() => {
    root.render(<PluginIcon pluginId="brand" tileId="status" svg={source} />);
  });

  const iconHost = host.querySelector("span");
  const svg = iconHost?.shadowRoot?.querySelector("svg");
  expect(iconHost?.hidden).toBe(false);
  expect(svg).not.toBeNull();
  expect(svg?.getAttribute("viewBox")).toBe("0 0 24 24");
  expect(svg?.getAttribute("width")).toBe("1em");
  expect(svg?.getAttribute("height")).toBe("1em");
  expect(svg?.getAttribute("focusable")).toBe("false");
  expect(svg?.querySelector("script")).toBeNull();
  expect(svg?.querySelector("[onclick], [style], [class]")).toBeNull();
  expect(svg?.querySelector("path")?.hasAttribute("fill")).toBe(false);
  expect(svg?.querySelector("circle")?.getAttribute("fill")).toBe(
    "url(#brand)",
  );
  expect(logged).toHaveLength(1);
  expect(logged[0]?.message).toContain("svg[style]");
  expect(logged[0]?.message).toContain("svg[onclick]");
  expect(logged[0]?.message).toContain("path[class]");
  expect(logged[0]?.message).toContain("path[fill]");
  expect(logged[0]?.options).toMatchObject({
    pluginId: "brand",
    fields: { tileId: "status", target: "icon" },
  });

  act(() => {
    root.render(
      <PluginIcon pluginId="brand" tileId="status" svg={`${source} `} />,
    );
  });
  expect(logged).toHaveLength(1);
});

test("renders nothing unless the sanitized input has exactly one SVG root", () => {
  for (const svg of [
    "<div><svg></svg></div>",
    "<svg></svg><svg></svg>",
    "plain text",
    `<svg>${"x".repeat(8 * 1024)}</svg>`,
  ]) {
    act(() => {
      root.render(<PluginIcon pluginId="brand" tileId="status" svg={svg} />);
    });
    expect(
      host.querySelector("span")?.shadowRoot?.querySelector("svg"),
      svg.slice(0, 80),
    ).toBeNull();
    expect(host.querySelector("span")?.hidden).toBe(true);
  }
  expect(logged).toHaveLength(2);
  expect(logged[0]?.message).toContain("iconSvg");
  expect(logged[1]?.message).toContain("iconSvg");
});

test("shares a folder PNG, prefers an explicit SVG, and updates after reload", () => {
  const dataUrl = "data:image/png;base64,aWNvbg==";
  const render = (svg?: string, image = dataUrl) => {
    act(() => {
      root.render(
        <PluginIcon
          pluginId="brand"
          tileId="status"
          svg={svg}
          dataUrl={image}
          fallback={<b>Fallback</b>}
        />,
      );
    });
  };
  render();
  const iconHost = host.querySelector("span");
  const img = () => iconHost?.shadowRoot?.querySelector("img");
  expect(img()?.getAttribute("src")).toBe(dataUrl);
  expect(img()?.width).toBe(128);
  expect(img()?.height).toBe(128);
  expect(img()?.alt).toBe("");
  expect(img()?.draggable).toBe(false);
  render("<svg><path d='M0 0h24v24z'/></svg>");
  expect(img()).toBeNull();
  expect(iconHost?.shadowRoot?.querySelector("svg")).not.toBeNull();
  render("<div>Invalid SVG</div>");
  expect(img()?.src).toBe(dataUrl);
  render(undefined, "data:image/png;base64,bmV3");
  expect(img()?.src).toBe("data:image/png;base64,bmV3");
  act(() => {
    img()?.dispatchEvent(new Event("error"));
  });
  expect(img()).toBeNull();
  expect(iconHost?.hidden).toBe(false);
  expect(iconHost?.shadowRoot?.querySelector("slot")).not.toBeNull();
  expect(logged.at(-1)?.options).toMatchObject({
    fields: {
      dropped: [
        {
          what: "icon",
          reason:
            "The plugin icon could not be displayed. Reload the plugin to refresh it.",
        },
      ],
    },
  });
  render(undefined, "");
  expect(img()).toBeNull();
});

test("never loads a remote URL, other media type or oversized image payload", () => {
  for (const dataUrl of [
    "https://example.com/icon.png",
    "data:image/svg+xml;base64,c3Zn",
    `data:image/png;base64,${"A".repeat(128 * 1024)}`,
  ]) {
    act(() => {
      root.render(
        <PluginIcon pluginId="brand" tileId="status" dataUrl={dataUrl} />,
      );
    });
    const iconHost = host.querySelector("span");
    expect(iconHost?.shadowRoot?.querySelector("img")).toBeNull();
    expect(iconHost?.hidden).toBe(true);
  }
  expect(logged).toHaveLength(1);
});
