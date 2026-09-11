// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, test, vi } from "vitest";

import { useZoneScroll } from "./useZoneScroll";

let resize: ResizeObserverCallback | undefined;
const observed = new Set<Element>();
const callbackObserver: ResizeObserver = {
  disconnect: () => undefined,
  observe: () => undefined,
  unobserve: () => undefined,
};

class TestResizeObserver implements ResizeObserver {
  constructor(callback: ResizeObserverCallback) {
    resize = callback;
  }

  observe(target: Element): void {
    observed.add(target);
  }

  unobserve(target: Element): void {
    observed.delete(target);
  }

  disconnect(): void {
    observed.clear();
  }
}

function Zone() {
  const ref = useZoneScroll();
  return (
    <div ref={ref}>
      <div data-child />
    </div>
  );
}

function notifyResize(): void {
  if (resize === undefined) {
    throw new Error("ResizeObserver was not installed");
  }
  resize([], callbackObserver);
}

afterEach(() => {
  vi.unstubAllGlobals();
  observed.clear();
  resize = undefined;
  document.body.replaceChildren();
});

test("remeasures when a direct child grows, shrinks, or is added", async () => {
  vi.stubGlobal("ResizeObserver", TestResizeObserver);
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  act(() => {
    root.render(<Zone />);
  });

  const zone = container.firstElementChild;
  if (!(zone instanceof HTMLDivElement)) throw new Error("missing zone");
  const child = zone.firstElementChild;
  if (!(child instanceof HTMLDivElement)) throw new Error("missing child");
  Object.defineProperty(zone, "clientWidth", { value: 100 });
  Object.defineProperty(child, "offsetLeft", {
    configurable: true,
    value: 0,
  });
  Object.defineProperty(child, "offsetWidth", {
    configurable: true,
    value: 140,
  });
  expect(observed).toEqual(new Set([zone, child]));

  notifyResize();
  expect(zone.hasAttribute("data-zone-overflowing")).toBe(true);
  Object.defineProperty(child, "offsetWidth", {
    configurable: true,
    value: 80,
  });
  notifyResize();
  expect(zone.hasAttribute("data-zone-overflowing")).toBe(false);

  // Right reserve padding is part of the transform-free scroll extent, and
  // a theme token can change it without any observed element resizing.
  zone.style.paddingInlineEnd = "30px";
  document.documentElement.style.setProperty("--sb-test-zone-gap", "1px");
  await act(async () => {
    await Promise.resolve();
  });
  expect(zone.hasAttribute("data-zone-overflowing")).toBe(true);

  zone.style.paddingInlineEnd = "0";
  Object.defineProperty(child, "offsetLeft", {
    configurable: true,
    value: 30,
  });
  document.documentElement.style.setProperty("--sb-test-zone-gap", "2px");
  await act(async () => {
    await Promise.resolve();
  });
  expect(zone.hasAttribute("data-zone-overflowing")).toBe(true);

  const added = document.createElement("div");
  zone.append(added);
  await act(async () => {
    await Promise.resolve();
  });
  expect(observed.has(added)).toBe(true);

  act(() => {
    root.unmount();
  });
  document.documentElement.style.removeProperty("--sb-test-zone-gap");
});
