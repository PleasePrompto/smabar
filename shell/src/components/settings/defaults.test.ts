import { expect, test } from "vitest";

import * as defaults from "./defaults";
import {
  BAR_DEFAULTS,
  BAR_TOKENS,
  DESIGN_DEFAULTS,
  DESIGN_TOKENS,
  SHORTCUTS_DEFAULTS,
  SHORTCUTS_TOKENS,
  SYSTEM_DEFAULTS,
  PLUGINS_DEFAULTS,
  PLUGINS_TOKENS,
} from "./defaults";
import type { ConfigWrite } from "./persist";

const asMap = (writes: readonly ConfigWrite[]) =>
  Object.fromEntries(writes.map(({ path, value }) => [path, value]));

test("each settings section maps to the config model defaults", () => {
  expect(asMap(BAR_DEFAULTS)).toEqual({
    "layout.monitor": null,
    "layout.position": "bottom",
    "layout.variant": "split",
    "layout.dividerRatio": 0.5,
    "layout.primaryZone": "plugins",
    "layout.width": "auto",
    "layout.margin": 10,
    "layout.maxWidth": 0,
    "layout.behavior": "reserve",
    "layout.yieldToFullscreen": true,
    zOrder: "top",
    "popups.enabled": true,
    "popups.position": "bottom-right",
  });
  expect(asMap(SHORTCUTS_DEFAULTS)).toEqual({
    "shortcuts.labels": "hidden",
    "shortcuts.iconSize": 24,
    "shortcuts.labelSize": 12,
    "shortcuts.pinned": [],
    "appearance.shortcutAlign": "center",
    "effects.hoverMagnify.enabled": true,
    "effects.hoverMagnify.scale": 1.2,
    "effects.hoverMagnify.neighbors": 2,
  });
  // Reset undoes both reversible off-switches — hidden tiles AND switched
  // off plugins — and touches no installed files.
  expect(asMap(PLUGINS_DEFAULTS)).toEqual({
    pluginsHidden: [],
    pluginsDeactivated: [],
    pluginOrder: [],
    "appearance.tileChrome": "card",
    "appearance.pluginAlign": "center",
    "effects.hoverPeek.enabled": true,
    "effects.hoverPeek.delayMs": 400,
  });
  expect(asMap(DESIGN_DEFAULTS)).toEqual({
    theme: "default",
    themeExportDir: "",
    "appearance.barChrome": "card",
  });
  expect(asMap(SYSTEM_DEFAULTS)).toEqual({
    language: "en",
    "mcp.enabled": true,
    "mcp.port": 7627,
    rendering: "auto",
  });
});

/**
 * The section a setting resets from must be the section it is SHOWN in, so
 * every path belongs to exactly one constant. This is the check that catches
 * a setting lost — or silently duplicated — while it moves between sections.
 */
test("no config path is reset by two sections, and none is orphaned", () => {
  const sections = [
    BAR_DEFAULTS,
    SHORTCUTS_DEFAULTS,
    PLUGINS_DEFAULTS,
    DESIGN_DEFAULTS,
    SYSTEM_DEFAULTS,
  ];
  const paths = sections.flatMap((writes) => writes.map((write) => write.path));
  expect(paths).toHaveLength(new Set(paths).size);

  // Every `*_DEFAULTS` export is one of the five above: a sixth would be a
  // section whose reset nothing ever runs.
  const exported = Object.keys(defaults).filter((name) =>
    name.endsWith("_DEFAULTS"),
  );
  expect(exported).toHaveLength(sections.length);
});

/**
 * Token overrides live in one map, so a section that emptied it would throw
 * away the other sections' choices. Each one names the keys it owns instead,
 * and those sets may not overlap either.
 */
test("each section owns its appearance tokens exclusively", () => {
  const owned = [
    ...BAR_TOKENS,
    ...SHORTCUTS_TOKENS,
    ...PLUGINS_TOKENS,
    ...DESIGN_TOKENS,
  ];
  expect(owned).toHaveLength(new Set(owned).size);
});
