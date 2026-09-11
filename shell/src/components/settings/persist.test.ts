// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));

vi.mock("../../ipc/call", () => ({ call: callMock }));

import {
  setConfig,
  setConfigDebounced,
  setConfigsSequentially,
} from "./persist";

beforeEach(() => {
  vi.stubGlobal("reportError", vi.fn());
});

afterEach(() => {
  vi.useRealTimers();
  callMock.mockReset();
  vi.unstubAllGlobals();
});

test("reset writes wait for the preceding config update", async () => {
  let releaseFirst: () => void = () => undefined;
  callMock
    .mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          releaseFirst = () => {
            resolve();
          };
        }),
    )
    .mockResolvedValue(undefined);

  const pending = setConfigsSequentially([
    { path: "layout.position", value: "bottom" },
    { path: "layout.width", value: "full" },
  ]);
  expect(callMock).toHaveBeenCalledTimes(1);
  releaseFirst();
  await pending;
  expect(callMock).toHaveBeenNthCalledWith(2, "update_config", {
    path: "layout.width",
    value: "full",
  });
});

test("a direct write cancels an older debounced write to the same path", async () => {
  vi.useFakeTimers();
  callMock.mockResolvedValue(undefined);
  setConfigDebounced("layout.maxWidth", 1_200);
  setConfig("layout.maxWidth", 0);
  await vi.runAllTimersAsync();
  expect(callMock).toHaveBeenCalledTimes(1);
  expect(callMock).toHaveBeenCalledWith("update_config", {
    path: "layout.maxWidth",
    value: 0,
  });
});
