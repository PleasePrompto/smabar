// @vitest-environment happy-dom
import { act, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EffectsConfig } from "../../store/bar";
import { pushNativePointerSample, pushPointerSample } from "./useAutohide";
import { useFisheye } from "./useFisheye";

const effects: EffectsConfig = {
  hoverMagnify: { enabled: true, scale: 1.25, neighbors: 2 },
  hoverPeek: { enabled: true, delayMs: 400 },
};

function Harness() {
  const ref = useRef<HTMLDivElement>(null);
  useFisheye(ref, effects, 0);
  return (
    <div ref={ref}>
      <button className="shortcut-tile" />
      <button className="shortcut-tile" />
    </div>
  );
}

let host: HTMLDivElement;
let root: Root;
let scheduledFrame: FrameRequestCallback | null;
let scheduledFrameCount: number;

beforeEach(() => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  scheduledFrame = null;
  scheduledFrameCount = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    scheduledFrame = callback;
    scheduledFrameCount += 1;
    return scheduledFrameCount;
  });
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root.render(<Harness />);
  });
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  pushNativePointerSample(0, 0);
  host.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = false;
});

test("reads every tile rect before writing magnify styles", () => {
  const zone = host.firstElementChild;
  if (!(zone instanceof HTMLElement)) throw new Error("zone missing");
  vi.spyOn(zone, "getBoundingClientRect").mockReturnValue(
    new DOMRect(0, 0, 100, 40),
  );

  const operations: string[] = [];
  const tiles = [...zone.querySelectorAll<HTMLElement>(".shortcut-tile")];
  for (const [index, tile] of tiles.entries()) {
    vi.spyOn(tile, "getBoundingClientRect").mockImplementation(() => {
      operations.push(`read:${String(index)}`);
      return new DOMRect(index * 40, 0, 40, 40);
    });
    vi.spyOn(tile.style, "setProperty").mockImplementation((property) => {
      if (property === "--magnify") {
        operations.push(`write:${String(index)}`);
      }
    });
  }

  act(() => {
    document.dispatchEvent(
      new MouseEvent("mousemove", { clientX: 20, clientY: 20 }),
    );
  });
  const callback = scheduledFrame;
  if (callback === null) throw new Error("frame missing");
  act(() => {
    callback(0);
  });

  expect(operations).toEqual(["read:0", "read:1", "write:0", "write:1"]);
});

test("native leave cancels pending magnification and rejects stale moves", () => {
  const zone = host.firstElementChild;
  if (!(zone instanceof HTMLElement)) throw new Error("zone missing");
  vi.spyOn(zone, "getBoundingClientRect").mockReturnValue(
    new DOMRect(0, 0, 100, 40),
  );
  const tile = zone.querySelector<HTMLElement>(".shortcut-tile");
  if (tile === null) throw new Error("tile missing");
  vi.spyOn(tile, "getBoundingClientRect").mockReturnValue(
    new DOMRect(0, 0, 40, 40),
  );

  act(() => {
    document.dispatchEvent(
      new MouseEvent("mousemove", { clientX: 20, clientY: 20 }),
    );
  });
  expect(scheduledFrame).not.toBeNull();
  expect(scheduledFrameCount).toBe(1);

  act(() => {
    pushNativePointerSample(Number.NaN, Number.NaN);
  });
  expect(document.documentElement.hasAttribute("data-sb-pointer-outside")).toBe(
    true,
  );

  act(() => {
    pushNativePointerSample(20, 20);
  });
  expect(document.documentElement.hasAttribute("data-sb-pointer-outside")).toBe(
    false,
  );
  const freshCallback = scheduledFrame;
  if (freshCallback === null) throw new Error("fresh frame missing");
  expect(scheduledFrameCount).toBe(2);
  act(() => {
    freshCallback(0);
  });
  expect(tile.style.getPropertyValue("--magnify")).not.toBe("");

  act(() => {
    pushNativePointerSample(Number.NaN, Number.NaN);
  });
  expect(tile.style.getPropertyValue("--magnify")).toBe("");

  scheduledFrame = null;
  act(() => {
    document.dispatchEvent(
      new MouseEvent("mousemove", { clientX: 20, clientY: 20 }),
    );
  });
  expect(scheduledFrame).toBeNull();
});

test("a native pointer sample outside the shortcut zone clears magnification", () => {
  const zone = host.firstElementChild;
  if (!(zone instanceof HTMLElement)) throw new Error("zone missing");
  vi.spyOn(zone, "getBoundingClientRect").mockReturnValue(
    new DOMRect(0, 0, 100, 40),
  );
  const tile = zone.querySelector<HTMLElement>(".shortcut-tile");
  if (tile === null) throw new Error("tile missing");
  vi.spyOn(tile, "getBoundingClientRect").mockReturnValue(
    new DOMRect(0, 0, 40, 40),
  );

  act(() => {
    document.dispatchEvent(
      new MouseEvent("mousemove", { clientX: 20, clientY: 20 }),
    );
  });
  const callback = scheduledFrame;
  if (callback === null) throw new Error("frame missing");
  act(() => {
    callback(0);
  });
  expect(tile.style.getPropertyValue("--magnify")).not.toBe("");

  act(() => {
    pushPointerSample(200, 200);
  });
  expect(tile.style.getPropertyValue("--magnify")).toBe("");
});
