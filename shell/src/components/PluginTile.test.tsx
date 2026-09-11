import { beforeEach, expect, test } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import { useSmabar } from "../store/bar";
import { tileChrome, PluginTile } from "./PluginTile";

beforeEach(() => {
  useSmabar.setState(useSmabar.getInitialState(), true);
});

function tile(chrome?: "card" | "flat"): string {
  return renderToStaticMarkup(
    <PluginTile
      tileId="plugin:weather:main"
      label="Weather"
      onTrigger={() => undefined}
      isActive={false}
      chrome={chrome}
    >
      21°
    </PluginTile>,
  );
}

test("tile chrome resolves global and manifest values and renders the result", () => {
  // data-tile-id is what the context menu resolves the tile through.
  expect(tile()).toContain('data-tile-id="plugin:weather:main"');
  expect(tile()).toContain('data-tile-chrome="flat"');
  expect(tile()).not.toContain("surface-tile");

  expect(tileChrome("card")).toBe("card");
  expect(tileChrome("flat")).toBe("flat");
  expect(tileChrome("flat", "card")).toBe("card");
  expect(tile("flat")).toContain('data-tile-chrome="flat"');
  expect(tile("flat")).not.toContain("surface-tile");
  expect(tile("card")).toContain("surface-tile");
});

test.each(["card", "flat"] as const)(
  "%s tiles expose the open flyout and keep its active chrome",
  (chrome) => {
    for (const isActive of [false, true]) {
      const markup = renderToStaticMarkup(
        <PluginTile
          tileId="plugin:weather:main"
          label="Weather"
          onTrigger={() => undefined}
          chrome={chrome}
          hasFlyout
          isActive={isActive}
        >
          21°
        </PluginTile>,
      );
      expect(markup).toContain(`aria-expanded="${String(isActive)}"`);
      expect(markup.includes("surface-tile-active")).toBe(isActive);
    }
    // A tile that only performs an action must not claim to expand a panel.
    expect(tile(chrome)).not.toContain("aria-expanded");
  },
);
