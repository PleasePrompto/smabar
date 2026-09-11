// @vitest-environment happy-dom
// The core rejects a 0×0 measure with "… size must be between 1 and 16384 CSS
// pixels"; a root that is not laid out yet must therefore never be reported.
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  hasLayout,
  reportContextMenuMeasure,
  reportFlyoutMeasure,
  reportTooltipMeasure,
} from "./overlay";

const { invokeMock, uiLogMock } = vi.hoisted(() => ({
  invokeMock: vi.fn<(command: string, args?: unknown) => Promise<unknown>>(() =>
    Promise.resolve(),
  ),
  uiLogMock: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));
vi.mock("./log", () => ({ uiLog: uiLogMock, reportError: vi.fn() }));

beforeEach(() => {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
  invokeMock.mockClear();
  uiLogMock.mockClear();
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

test("hasLayout requires both dimensions", () => {
  expect(hasLayout({ width: 1, height: 1 })).toBe(true);
  expect(hasLayout({ width: 0, height: 40 })).toBe(false);
  expect(hasLayout({ width: 320, height: 0 })).toBe(false);
});

test("a measure without layout never reaches the core", async () => {
  await reportFlyoutMeasure({
    generation: 3,
    width: 0,
    height: 0,
    inset: 16,
    gap: 14,
    pointerReserve: 0,
  });
  await reportContextMenuMeasure({
    generation: 4,
    width: 120,
    height: 0,
    inset: 8,
  });
  await reportTooltipMeasure({
    generation: 5,
    width: 0,
    height: 20,
    inset: 8,
    gap: 6,
  });

  expect(invokeMock).not.toHaveBeenCalled();
  expect(uiLogMock).toHaveBeenCalledTimes(3);
  expect(uiLogMock).toHaveBeenCalledWith(
    "debug",
    "skipped a flyout measure without layout",
    { fields: { surface: "flyout", generation: 3 } },
  );
});

test("a laid-out measure is forwarded unchanged", async () => {
  const measure = {
    generation: 7,
    width: 320,
    height: 180,
    inset: 16,
    gap: 14,
    pointerReserve: 0,
  };
  await reportFlyoutMeasure(measure);
  expect(invokeMock).toHaveBeenCalledWith("measure_flyout", { measure });
  expect(uiLogMock).not.toHaveBeenCalled();
});
