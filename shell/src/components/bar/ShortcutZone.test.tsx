// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";

import { ShortcutItem } from "./ShortcutZone";

test("a separator is neither launchable nor a magnified shortcut tile", () => {
  const html = renderToStaticMarkup(
    <ShortcutItem
      shortcut={{ id: "separator-1", label: "", icons: [], separator: true }}
      labels="right"
    />,
  );

  expect(html).toContain("shortcut-separator");
  expect(html).not.toContain("<button");
  expect(html).not.toContain("shortcut-tile");
});

test("a website pin renders its best icon candidate as the tile icon", () => {
  const html = renderToStaticMarkup(
    <ShortcutItem
      shortcut={{
        id: "sc-web",
        label: "bild.de",
        icons: [
          "https://www.bild.de/apple-touch-icon.png",
          "https://www.bild.de/favicon.ico",
        ],
        separator: false,
      }}
      labels="right"
    />,
  );

  expect(html).toContain('src="https://www.bild.de/apple-touch-icon.png"');
  expect(html).not.toContain("shortcut-initial");
});

test("a downloaded favicon replaces exhausted remote candidates without repinning", () => {
  const actEnvironment = globalThis as typeof globalThis & {
    IS_REACT_ACT_ENVIRONMENT?: boolean;
  };
  const previousActEnvironment = actEnvironment.IS_REACT_ACT_ENVIRONMENT;
  actEnvironment.IS_REACT_ACT_ENVIRONMENT = true;
  const host = document.createElement("div");
  const root = createRoot(host);
  const remote = [
    "https://example.com/apple-touch-icon.png",
    "https://example.com/favicon.ico",
  ];
  const render = (icons: string[]) => {
    root.render(
      <ShortcutItem
        shortcut={{ id: "same-pin", label: "Example", icons, separator: false }}
        labels="hidden"
      />,
    );
  };
  try {
    act(() => {
      render(remote);
    });
    for (const url of remote) {
      expect(host.querySelector("img")?.getAttribute("src")).toBe(url);
      act(() => {
        host.querySelector("img")?.dispatchEvent(new Event("error"));
      });
    }
    expect(host.querySelector(".shortcut-initial")?.textContent).toBe("E");
    const cached = "data:image/png;base64,iVBORw==";
    act(() => {
      render([cached, ...remote]);
    });
    expect(host.querySelector("img")?.getAttribute("src")).toBe(cached);
    expect(host.querySelector(".shortcut-initial")).toBeNull();
  } finally {
    act(() => {
      root.unmount();
    });
    actEnvironment.IS_REACT_ACT_ENVIRONMENT = previousActEnvironment;
  }
});
