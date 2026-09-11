import { expect, test } from "vitest";

import type { EffectsConfig } from "../../store/bar";
import {
  coverRowHeightExpr,
  fisheyeScale,
  magnifyReserve,
  rowHeightExpr,
  tallestCover,
} from "./metrics";

const effects = (enabled: boolean, scale: number): EffectsConfig => ({
  hoverMagnify: { enabled, scale, neighbors: 2 },
  hoverPeek: { enabled: true, delayMs: 400 },
});

test("the row grows to the tallest cover and keeps the configured padding", () => {
  expect(coverRowHeightExpr("3rem", 0)).toBe("3rem");
  expect(coverRowHeightExpr("3rem", Number.NaN)).toBe("3rem");
  expect(coverRowHeightExpr("3rem", 41.4)).toBe(
    "max(3rem, calc(42px + 2 * var(--sb-tile-pad-y, 0.375rem) + 2 * var(--sb-border-width, 1px) + 2 * var(--sb-bar-pad-y, 0.375rem) + 2 * var(--sb-bar-border-width, var(--sb-border-width, 1px))))",
  );
  expect(tallestCover({})).toBe(0);
  expect(tallestCover({ a: 20, b: 44, c: 30 })).toBe(44);
});

test("rowHeightExpr grows the row for below-labels and clamps the sizes", () => {
  // Everything is rem so the global size slider (root font-size) reaches it —
  // including the icon/label gap, which must keep matching Tailwind's gap-1.
  expect(rowHeightExpr({ labels: "below", iconSize: 48, labelSize: 12 })).toBe(
    "calc(4rem + 2 * var(--sb-bar-pad-y, 0.375rem) + 2 * var(--sb-bar-border-width, var(--sb-border-width, 1px)))",
  );
  // 999 clamps to 64, 99 to 16.
  expect(rowHeightExpr({ labels: "below", iconSize: 999, labelSize: 99 })).toBe(
    "calc(5.25rem + 2 * var(--sb-bar-pad-y, 0.375rem) + 2 * var(--sb-bar-border-width, var(--sb-border-width, 1px)))",
  );
  for (const labels of ["right", "hidden"] as const) {
    expect(rowHeightExpr({ labels, iconSize: 40, labelSize: 12 })).toBe(
      "max(var(--sb-bar-height, 3.25rem), calc(2.5rem + 2 * var(--sb-bar-pad-y, 0.375rem) + 2 * var(--sb-bar-border-width, var(--sb-border-width, 1px))))",
    );
  }
  // Card tile chrome wraps the icon in --sb-tile-pad-y padding (bar.css), so
  // the floor budgets it — otherwise big icons would push the tile past the
  // row and the zone would clip it.
  expect(
    rowHeightExpr({ labels: "hidden", iconSize: 40, labelSize: 12 }, true),
  ).toBe(
    "max(var(--sb-bar-height, 3.25rem), calc(2.5rem + 2 * var(--sb-tile-pad-y, 0.375rem) + 2 * var(--sb-border-width, 1px) + 2 * var(--sb-bar-pad-y, 0.375rem) + 2 * var(--sb-bar-border-width, var(--sb-border-width, 1px))))",
  );
  expect(
    rowHeightExpr({ labels: "below", iconSize: 48, labelSize: 12 }, true),
  ).toBe(
    "calc(4rem + 2 * var(--sb-tile-pad-y, 0.375rem) + 2 * var(--sb-border-width, 1px) + 2 * var(--sb-bar-pad-y, 0.375rem) + 2 * var(--sb-bar-border-width, var(--sb-border-width, 1px)))",
  );
});

test("magnifyReserve covers the whole tile's growth and is 0 when off", () => {
  // Icon-only height: 40 × (1.5 − 1) = 20.
  expect(
    magnifyReserve(
      { labels: "right", iconSize: 40, labelSize: 12 },
      effects(true, 1.5),
    ),
  ).toBe(20);
  // Below-labels: (40 + 4 + 12) × 0.5 = 28.
  expect(
    magnifyReserve(
      { labels: "below", iconSize: 40, labelSize: 12 },
      effects(true, 1.5),
    ),
  ).toBe(28);
  expect(
    magnifyReserve(
      { labels: "right", iconSize: 40, labelSize: 12 },
      effects(false, 1.5),
    ),
  ).toBe(0);
  // Card chrome: the 20px padding-and-rounding allowance grows the covered
  // tile height — (40 + 20) × 0.5 = 30.
  expect(
    magnifyReserve(
      { labels: "right", iconSize: 40, labelSize: 12 },
      effects(true, 1.5),
      true,
    ),
  ).toBe(30);
  // A user theme may make the card much taller; the measured border-box is
  // the source of truth once the tile has mounted.
  expect(
    magnifyReserve(
      { labels: "right", iconSize: 40, labelSize: 12 },
      effects(true, 1.5),
      true,
      104,
    ),
  ).toBe(52);
});

test("fisheyeScale peaks at the cursor and falls to 1 at the radius", () => {
  expect(fisheyeScale(0, 40, 1.5, 2)).toBeCloseTo(1.5);
  // Radius = 40 × 2.5 = 100: outside → exactly 1.
  expect(fisheyeScale(100, 40, 1.5, 2)).toBe(1);
  expect(fisheyeScale(-100, 40, 1.5, 2)).toBe(1);
  // Halfway the cosine falloff gives half the boost.
  expect(fisheyeScale(50, 40, 1.5, 2)).toBeCloseTo(1.25);
  // Monotonically decreasing with distance.
  const scales = [0, 20, 40, 60, 80, 100].map((d) =>
    fisheyeScale(d, 40, 1.5, 2),
  );
  for (let i = 1; i < scales.length; i += 1) {
    const current = scales[i] ?? 0;
    const previous = scales[i - 1] ?? 0;
    expect(current).toBeLessThanOrEqual(previous);
  }
});

test("fisheyeScale with 0 neighbors only covers the hovered tile", () => {
  // Radius = half an item width: the neighbor center (1 width away) is out.
  expect(fisheyeScale(0, 40, 1.5, 0)).toBeCloseTo(1.5);
  expect(fisheyeScale(19, 40, 1.5, 0)).toBeGreaterThan(1);
  expect(fisheyeScale(40, 40, 1.5, 0)).toBe(1);
});

test("fisheyeScale clamps neighbors and degrades safely", () => {
  // 99 neighbors clamp to 3 → radius 3.5 widths.
  expect(fisheyeScale(3.4 * 40, 40, 1.5, 99)).toBeGreaterThan(1);
  expect(fisheyeScale(3.5 * 40, 40, 1.5, 99)).toBe(1);
  // Degenerate inputs never blow up.
  expect(fisheyeScale(10, 0, 1.5, 2)).toBe(1);
  expect(fisheyeScale(10, 40, 1, 2)).toBe(1);
});
