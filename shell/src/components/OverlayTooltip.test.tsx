// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { OverlayTooltip } from "./OverlayTooltip";

type Listener = (event: { payload: unknown }) => void;

const { listeners, reportMeasureMock } = vi.hoisted(() => ({
  listeners: new Map<string, Listener>(),
  reportMeasureMock: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, listener: Listener) => {
    listeners.set(name, listener);
    return Promise.resolve(() => undefined);
  }),
}));

vi.mock("../ipc/overlay", () => ({
  reportTooltipMeasure: reportMeasureMock,
}));

let host: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  listeners.clear();
  reportMeasureMock.mockClear();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(<OverlayTooltip />);
    await Promise.resolve();
  });
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

test("same-generation text updates keep the visible placement", () => {
  emit("surface-tooltip", { generation: 7, text: "CPU 5%" });
  emit("tooltip-placement", {
    generation: 7,
    side: "top",
    x: 10,
    y: 20,
  });
  expect(tooltip().style.visibility).toBe("visible");

  emit("surface-tooltip", { generation: 7, text: "CPU 25%" });

  expect(tooltip().textContent).toBe("CPU 25%");
  expect(tooltip().style.visibility).toBe("visible");
});

function emit(name: string, payload: unknown): void {
  const listener = listeners.get(name);
  if (listener === undefined) throw new Error(`missing ${name} listener`);
  act(() => {
    listener({ payload });
  });
}

function tooltip(): HTMLDivElement {
  const element = host.querySelector<HTMLDivElement>(".overlay-tooltip");
  if (element === null) throw new Error("tooltip missing");
  return element;
}
