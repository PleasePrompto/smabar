import { expect, test, vi } from "vitest";

import { cleanupListeners } from "./listeners";

const { reportErrorMock } = vi.hoisted(() => ({
  reportErrorMock: vi.fn(),
}));

vi.mock("./log", () => ({ reportError: reportErrorMock }));

test("cleans up successful listeners when siblings fail or resolve late", async () => {
  const firstStop = vi.fn();
  const lateStop = vi.fn();
  const failure = new Error("registration failed");
  let resolveLate: ((stop: () => void) => void) | undefined;
  const late = new Promise<() => void>((resolve) => {
    resolveLate = resolve;
  });

  const cleanup = cleanupListeners([
    Promise.resolve(firstStop),
    Promise.reject(failure),
    late,
  ]);
  await Promise.resolve();

  cleanup();
  resolveLate?.(lateStop);
  await Promise.resolve();

  expect(firstStop).toHaveBeenCalledOnce();
  expect(lateStop).toHaveBeenCalledOnce();
  expect(reportErrorMock).toHaveBeenCalledWith(failure);
});
