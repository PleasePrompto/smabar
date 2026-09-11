// @vitest-environment happy-dom
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../../store/bar";
import { collectTargets } from "../../ipc/captureTargets";
import { collectRects } from "../../ipc/inputShape";
import { BarShell } from "./BarShell";
import { AUTOHIDE_HIDE_DELAY_MS, pushNativePointerSample } from "./useAutohide";

const { openSettingsMock } = vi.hoisted(() => ({
  openSettingsMock: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../ipc/surface", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../ipc/surface")>()),
  openSettings: openSettingsMock,
}));

beforeEach(() => {
  document.body.replaceChildren();
  useSmabar.setState(useSmabar.getInitialState(), true);
});

let root: Root | null = null;

afterEach(() => {
  root?.unmount();
  root = null;
  pushNativePointerSample(0, 0);
  vi.useRealTimers();
  vi.restoreAllMocks();
});

function renderBar(): void {
  root?.unmount();
  document.body.replaceChildren();
  const host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  flushSync(() => {
    root?.render(<BarShell />);
  });
}

test("full width is centered and capped while auto width ignores maxWidth", () => {
  const state = useSmabar.getState();
  state.setLayout({ ...state.layout, margin: 0, maxWidth: 120 });
  renderBar();
  const fullDock = document.querySelector<HTMLElement>("[data-bar-dock]");
  expect(fullDock?.style.width).toBe("100%");
  // Both caps apply: the explicit one, and the margins the bar sits inside.
  expect(fullDock?.style.maxWidth).toBe(
    "min(400px, calc(var(--sb-work-area-width, 100vw) - 0px))",
  );
  expect(fullDock?.style.marginInline).toBe("auto");

  useSmabar.getState().setLayout({
    ...useSmabar.getState().layout,
    width: "auto",
    margin: 12,
    maxWidth: 900,
  });
  renderBar();
  const autoDock = document.querySelector<HTMLElement>("[data-bar-dock]");
  expect(autoDock?.style.width).toBe("fit-content");
  expect(autoDock?.style.maxWidth).toBe(
    "calc(var(--sb-work-area-width, 100vw) - 24px)",
  );
});

test("a full-width bar can be inset and rounded like a floating one", () => {
  const state = useSmabar.getState();
  state.setLayout({
    ...state.layout,
    width: "full",
    position: "bottom",
    margin: 16,
    maxWidth: 0,
  });
  renderBar();
  const dock = document.querySelector<HTMLElement>("[data-bar-dock]");
  // The margin insets it on every side, so it no longer touches the screen.
  expect(dock?.style.maxWidth).toBe(
    "calc(var(--sb-work-area-width, 100vw) - 32px)",
  );
  expect(dock?.style.marginBottom).toBe("16px");
  // The rounding itself is not asserted here: happy-dom drops a
  // `border-radius: var(…)` on the way into the style attribute (real
  // browsers keep it), so this would test the fake DOM, not the bar. It is
  // one unconditional declaration now — verified in the real window.
});

test("autohide leaves an input hotzone and stays revealed for open surfaces", () => {
  const state = useSmabar.getState();
  state.setLayout({ ...state.layout, behavior: "autohide" });
  renderBar();

  const hiddenSurface = document.querySelector("[data-autohide-surface]");
  const hotzone = document.querySelector("[data-autohide-hotzone]");
  expect(document.querySelector("[data-autohide='hidden']")).not.toBeNull();
  expect(hiddenSurface?.hasAttribute("data-input-region")).toBe(true);
  expect(hotzone?.hasAttribute("data-input-region")).toBe(true);
  expect(hotzone?.getAttribute("data-autohide-region")).toBe("activation");
  // The strip belongs to the bounded native surface, not the dock content
  // that remains static while the whole window moves.
  expect(hotzone?.parentElement).not.toBe(hiddenSurface);
  expect(hotzone?.parentElement?.hasAttribute("data-autohide")).toBe(true);

  useSmabar.getState().setOverlayOpen(true);
  renderBar();
  const revealedSurface = document.querySelector("[data-autohide-surface]");
  expect(document.querySelector("[data-autohide='revealed']")).not.toBeNull();
  expect(revealedSurface?.hasAttribute("data-input-region")).toBe(true);
  // The gap and activation strip both stay in place throughout movement.
  expect(
    document
      .querySelector("[data-autohide-gap]")
      ?.getAttribute("data-autohide-region"),
  ).toBe("edge");
});

test("native presence reveals without repainting and ignores stale moves after leaving", () => {
  vi.useFakeTimers();
  const state = useSmabar.getState();
  state.setLayout({ ...state.layout, behavior: "autohide" });
  renderBar();

  const hotzone = document.querySelector<HTMLElement>(
    "[data-autohide-hotzone]",
  );
  if (hotzone === null) throw new Error("autohide hotzone missing");
  vi.spyOn(hotzone, "getBoundingClientRect").mockReturnValue(
    DOMRect.fromRect({ x: 0, y: 90, width: 100, height: 10 }),
  );
  // Off-screen WebKit surfaces can stop repainting. Enter/leave still work.
  vi.spyOn(window, "requestAnimationFrame").mockReturnValue(1);

  flushSync(() => {
    pushNativePointerSample(50, 95);
  });
  expect(document.querySelector("[data-autohide='revealed']")).not.toBeNull();

  pushNativePointerSample(Number.NaN, Number.NaN);
  document.dispatchEvent(
    new MouseEvent("mousemove", { clientX: 50, clientY: 95 }),
  );
  flushSync(() => {
    vi.advanceTimersByTime(AUTOHIDE_HIDE_DELAY_MS);
  });
  expect(document.querySelector("[data-autohide='hidden']")).not.toBeNull();

  flushSync(() => {
    pushNativePointerSample(50, 95);
  });
  expect(document.querySelector("[data-autohide='revealed']")).not.toBeNull();
});

test("native autohide preserves its input regions while the window moves", () => {
  const state = useSmabar.getState();
  state.setLayout({ ...state.layout, behavior: "autohide" });
  renderBar();
  for (const element of document.querySelectorAll(
    "[data-autohide-surface], [data-autohide-hotzone], [data-autohide-gap]",
  )) {
    vi.spyOn(element, "getBoundingClientRect").mockReturnValue(
      DOMRect.fromRect({ x: 0, y: 10, width: 100, height: 30 }),
    );
  }
  const hiddenRegions = collectRects();
  flushSync(() => {
    state.setOverlayOpen(true);
  });
  expect(collectRects()).toEqual(hiddenRegions);
  flushSync(() => {
    state.setOverlayOpen(false);
  });
  expect(collectRects()).toEqual(hiddenRegions);
});

test("Settings holds autohide through mode changes and releases it without pointer re-entry", () => {
  vi.useFakeTimers();
  useSmabar.setState({ settingsOpen: true });
  renderBar();
  const state = useSmabar.getState();
  flushSync(() => {
    state.setLayout({ ...state.layout, behavior: "autohide" });
  });
  flushSync(() => {
    pushNativePointerSample(Number.NaN, Number.NaN);
    vi.advanceTimersByTime(AUTOHIDE_HIDE_DELAY_MS);
  });
  expect(document.querySelector("[data-autohide='revealed']")).not.toBeNull();
  flushSync(() => {
    useSmabar.setState({ settingsOpen: false });
  });
  flushSync(() => vi.advanceTimersByTime(AUTOHIDE_HIDE_DELAY_MS));
  expect(document.querySelector("[data-autohide='hidden']")).not.toBeNull();
});

test("a pinned flyout holds autohide when a tile update resizes the bar", () => {
  vi.useFakeTimers();
  const state = useSmabar.getState();
  state.setLayout({ ...state.layout, behavior: "autohide" });
  useSmabar.setState({
    openFlyout: "plugin:clock:clock",
    flyoutMode: "pinned",
    triggerRect: { left: 20, top: 10, width: 100, height: 38 },
  });
  renderBar();
  flushSync(() => {
    pushNativePointerSample(Number.NaN, Number.NaN);
    // Toggling clock seconds/style changes its tile width and native bar size.
    window.dispatchEvent(new Event("resize"));
    vi.advanceTimersByTime(AUTOHIDE_HIDE_DELAY_MS);
  });
  expect(useSmabar.getState().openFlyout).toBe("plugin:clock:clock");
  expect(document.querySelector("[data-autohide='revealed']")).not.toBeNull();
  flushSync(() => {
    useSmabar.getState().closeFlyout();
  });
  expect(document.querySelector("[data-autohide='hidden']")).not.toBeNull();
});

test("unaccepted terms reduce the bar to one tile that opens the legal settings", () => {
  useSmabar.setState({ legalRequired: true });
  renderBar();
  // One row, no zone: nothing but the way to the terms is on offer.
  expect(document.querySelectorAll("[data-bar-root]")).toHaveLength(1);
  expect(document.querySelector("[data-zone-align]")).toBeNull();
  expect(
    document.querySelector<HTMLElement>("[data-bar-dock]")?.style.width,
  ).toBe("fit-content");
  expect(
    document.querySelector<HTMLElement>("[data-bar-root]")?.style.marginInline,
  ).toBe("auto");
  const tile = document.querySelector<HTMLButtonElement>("[data-legal-gate]");
  if (tile === null) throw new Error("legal gate tile missing");
  // Inside the row's input region, or X11 would let clicks fall through.
  expect(tile.closest("[data-input-region]")).not.toBeNull();
  expect(tile.getAttribute("aria-label")).toBe(
    "Accept the terms to start using smabar",
  );
  tile.click();
  expect(openSettingsMock).toHaveBeenCalledWith("legal");

  useSmabar.getState().setLegalRequired(false);
  renderBar();
  expect(document.querySelector("[data-legal-gate]")).toBeNull();
  expect(document.querySelector("[data-zone-align]")).not.toBeNull();
});

test("solo shows its second row inside the bar only while toggled open", () => {
  const state = useSmabar.getState();
  state.setLayout({ ...state.layout, variant: "solo", width: "auto" });
  state.setOverlayOpen(false);
  renderBar();
  // Both rows exist at all times: the secondary is pre-rendered and hidden,
  // inert and without an input region, so toggling never resizes the window.
  const hidden = () =>
    [...document.querySelectorAll<HTMLElement>("[data-bar-root]")].map((row) =>
      row.hasAttribute("data-bar-hidden"),
    );
  expect(hidden()).toEqual([true, false]);
  // Alone, the visible row hugs its content; with both shown it spans the dock.
  const primaryRow = () =>
    document.querySelector<HTMLElement>(
      "[data-bar-root]:not([data-bar-hidden])",
    );
  expect(primaryRow()?.style.width).toBe("fit-content");
  const secondary = document.querySelector<HTMLElement>("[data-bar-hidden]");
  expect(secondary?.style.visibility).toBe("hidden");
  expect(secondary?.hasAttribute("data-input-region")).toBe(false);
  expect(secondary?.hasAttribute("inert")).toBe(true);
  useSmabar.getState().setOverlayOpen(true);
  renderBar();
  expect(hidden()).toEqual([false, false]);
  expect(
    document.querySelectorAll("[data-bar-root][data-input-region]"),
  ).toHaveLength(2);
  expect(primaryRow()?.style.width).toBe("");
  useSmabar.getState().setOverlayOpen(false);
  renderBar();
  expect(hidden()).toEqual([true, false]);
});

test.each([
  ["top", "shortcuts"],
  ["top", "plugins"],
  ["bottom", "shortcuts"],
  ["bottom", "plugins"],
] as const)(
  "overlay capture selects only the visible secondary solo row at %s with %s primary",
  (position, primaryZone) => {
    const state = useSmabar.getState();
    state.setLayout({
      ...state.layout,
      variant: "solo",
      position,
      primaryZone,
    });
    renderBar();
    expect(collectTargets().has("overlay")).toBe(false);

    flushSync(() => {
      state.setOverlayOpen(true);
    });
    const rows = document.querySelectorAll("[data-bar-root]");
    const target = collectTargets().get("overlay");
    expect(target).toBe(rows[position === "top" ? 1 : 0]);
    expect(target).not.toBe(collectTargets().get("bar"));
    expect(target?.hasAttribute("data-bar-hidden")).toBe(false);

    flushSync(() => {
      state.setOverlayOpen(false);
    });
    expect(collectTargets().has("overlay")).toBe(false);
    for (const variant of ["split", "rows"] as const) {
      flushSync(() => {
        state.setLayout({ ...state.layout, variant });
        state.setOverlayOpen(true);
      });
      expect(collectTargets().has("overlay")).toBe(false);
    }
  },
);
