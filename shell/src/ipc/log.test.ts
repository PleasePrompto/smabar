// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import {
  DEDUPE_WINDOW_MS,
  describeError,
  reportError,
  resetUiLog,
  uiLog,
} from "./log";

const calls: { command: string; args?: Record<string, unknown> }[] = [];
let fail = false;

vi.mock("./call", () => ({
  call: (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args });
    return fail ? Promise.reject(new Error("ipc down")) : Promise.resolve(null);
  },
}));

beforeEach(() => {
  calls.length = 0;
  fail = false;
  resetUiLog();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

test("a shell message reaches the core with its level and routing", () => {
  uiLog("warn", "flyout measured nothing");
  expect(calls).toEqual([
    {
      command: "ui_log",
      args: {
        level: "warn",
        message: "flyout measured nothing",
        fields: null,
        pluginId: null,
      },
    },
  ]);
});

test("a plugin id routes the entry into that plugin's own log", () => {
  uiLog("warn", "dropped <form>", { pluginId: "clock", fields: { count: 2 } });
  expect(calls[0]?.args?.pluginId).toBe("clock");
  expect(calls[0]?.args?.fields).toEqual({ count: 2 });
});

test("a render loop cannot flood the log", () => {
  for (let i = 0; i < 50; i += 1) uiLog("warn", "same problem");
  expect(calls).toHaveLength(1);

  vi.advanceTimersByTime(DEDUPE_WINDOW_MS + 1);
  uiLog("warn", "same problem");
  expect(calls).toHaveLength(2);
});

test("messages that differ are not deduplicated away", () => {
  uiLog("warn", "problem A");
  uiLog("warn", "problem B");
  uiLog("warn", "problem A", { pluginId: "clock" });
  expect(calls).toHaveLength(3);
});

test("a failing ui_log disables shipping instead of looping", async () => {
  fail = true;
  uiLog("error", "first");
  await Promise.resolve();
  await Promise.resolve();
  fail = false;
  uiLog("error", "second");
  expect(calls.map((entry) => entry.args?.message)).toEqual(["first"]);
});

test("reportError keeps the devtools behaviour and logs centrally", () => {
  const seen: unknown[] = [];
  const original = globalThis.reportError;
  globalThis.reportError = (error: unknown) => seen.push(error);
  try {
    const boom = new Error("boom");
    reportError(boom);
    expect(seen).toEqual([boom]);
  } finally {
    globalThis.reportError = original;
  }
  expect(calls[0]?.args?.level).toBe("error");
  expect(String(calls[0]?.args?.message)).toContain("boom");
});

test("anything a catch can receive turns into one readable line", () => {
  expect(describeError(new Error("nope"))).toContain("nope");
  expect(describeError("plain string")).toBe("plain string");
  expect(describeError({ code: 7 })).toBe('{"code":7}');
  expect(describeError(undefined)).toBe("undefined");
  expect(describeError(7n)).toBe("7");
  const circular: { self?: unknown } = {};
  circular.self = circular;
  expect(describeError(circular)).toBe("[object Object]");
});
