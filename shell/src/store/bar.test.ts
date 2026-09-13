// @vitest-environment happy-dom
import { beforeEach, expect, test, vi } from "vitest";

import {
  DIVIDER_DEFAULT,
  clampDividerRatio,
  clampHoverPeekDelay,
  clampMaxWidth,
  flyoutDirection,
  isRenderableRect,
  magnifyScale,
  shortcutDisplayLabel,
  useSmabar,
  type EffectsConfig,
} from "./bar";

test("special shortcut labels follow the locale until the user overrides them", () => {
  const shortcut = {
    id: "sc-computer",
    label: "Computer",
    icons: [],
    separator: false,
  };
  const translate = (key: string) =>
    key === "settings.shortcuts.specialComputer" ? "Dieser PC" : key;
  expect(
    shortcutDisplayLabel(
      shortcut,
      { id: shortcut.id, special: "computer" },
      translate,
    ),
  ).toBe("Dieser PC");
  expect(
    shortcutDisplayLabel(
      { ...shortcut, label: "Arbeitsplatz" },
      { id: shortcut.id, special: "computer", label: "Arbeitsplatz" },
      translate,
    ),
  ).toBe("Arbeitsplatz");
});

const rectA = { left: 10, top: 20, width: 30, height: 40 };
const rectB = { left: 50, top: 60, width: 70, height: 80 };

const effects = (enabled: boolean, scale: number): EffectsConfig => ({
  hoverMagnify: { enabled, scale, neighbors: 2 },
  hoverPeek: { enabled: true, delayMs: 400 },
});

beforeEach(() => {
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    value: 1_000,
  });
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: 800,
  });
  useSmabar.setState(useSmabar.getInitialState(), true);
});

test("clampDividerRatio keeps the ratio inside 0.15–0.85", () => {
  expect(clampDividerRatio(0.35)).toBe(0.35);
  expect(clampDividerRatio(0)).toBe(0.15);
  expect(clampDividerRatio(1)).toBe(0.85);
  expect(clampDividerRatio(Number.NaN)).toBe(DIVIDER_DEFAULT);
});

test("clampHoverPeekDelay rounds and clamps to 100–2000 ms", () => {
  expect(clampHoverPeekDelay(20)).toBe(100);
  expect(clampHoverPeekDelay(400.4)).toBe(400);
  expect(clampHoverPeekDelay(5_000)).toBe(2_000);
  expect(clampHoverPeekDelay(Number.NaN)).toBe(400);
});

test("clampMaxWidth keeps zero unlimited and caps nonzero values", () => {
  expect(clampMaxWidth(0, 1_920)).toBe(0);
  expect(clampMaxWidth(120, 1_920)).toBe(400);
  expect(clampMaxWidth(900.6, 1_920)).toBe(901);
  expect(clampMaxWidth(900, 720)).toBe(720);
  expect(clampMaxWidth(400, 320)).toBe(320);
  expect(clampMaxWidth(Number.NaN, 1_920)).toBe(0);
});

test("magnifyScale is 1 when disabled, else clamped to 1.0–1.6", () => {
  expect(magnifyScale(effects(false, 1.4))).toBe(1);
  expect(magnifyScale(effects(true, 1.4))).toBe(1.4);
  expect(magnifyScale(effects(true, 3))).toBe(1.6);
  expect(magnifyScale(effects(true, 0.5))).toBe(1);
  expect(magnifyScale(effects(true, Number.NaN))).toBe(1);
});

test("flyoutDirection opens away from the trigger's window half", () => {
  // trigger center at y=40 of an 800px window → upper half → down
  expect(flyoutDirection(rectA, 800)).toBe("down");
  // same trigger in a 100px window → center y=40 < 50 → still down
  expect(flyoutDirection(rectA, 100)).toBe("down");
  // trigger center at y=100 of a 150px window → lower half → up
  expect(flyoutDirection(rectB, 150)).toBe("up");
  // exact center counts as the lower half → up
  expect(
    flyoutDirection({ left: 0, top: 40, width: 10, height: 20 }, 100),
  ).toBe("up");
});

test("toggleFlyout opens a flyout with its trigger rect", () => {
  useSmabar.getState().toggleFlyout("weather", rectA);
  expect(useSmabar.getState().openFlyout).toBe("weather");
  expect(useSmabar.getState().triggerRect).toEqual(rectA);
  expect(useSmabar.getState().flyoutMode).toBe("pinned");
});

test("isRenderableRect refuses anchors that would clamp into the corner", () => {
  expect(
    isRenderableRect({ left: 0, top: 0, width: 24, height: 24 }, 1_000, 800),
  ).toBe(true);
  // Hidden, detached, or mid-unmount elements measure all zeros.
  expect(
    isRenderableRect({ left: 0, top: 0, width: 0, height: 0 }, 1_000, 800),
  ).toBe(false);
  expect(
    isRenderableRect({ left: 10, top: 10, width: 0, height: 40 }, 1_000, 800),
  ).toBe(false);
  expect(
    isRenderableRect({ left: 10, top: 10, width: 40, height: 0 }, 1_000, 800),
  ).toBe(false);
  // NaN/Infinity leak through arithmetic into CSS.
  expect(
    isRenderableRect(
      { left: Number.NaN, top: 0, width: 40, height: 40 },
      1_000,
      800,
    ),
  ).toBe(false);
  expect(
    isRenderableRect(
      {
        left: 0,
        top: Number.POSITIVE_INFINITY,
        width: 40,
        height: 40,
      },
      1_000,
      800,
    ),
  ).toBe(false);
  // Positive dimensions are not enough: an autohidden bar can leave only
  // its 8 px reveal strip in the viewport while the tile center is outside.
  expect(
    isRenderableRect({ left: 10, top: 792, width: 40, height: 20 }, 1_000, 800),
  ).toBe(false);
  expect(
    isRenderableRect(
      { left: 1_010, top: 10, width: 40, height: 20 },
      1_000,
      800,
    ),
  ).toBe(false);
});

test("flyouts never open on a degenerate anchor rect", () => {
  const dead = { left: 0, top: 0, width: 0, height: 0 };
  useSmabar.getState().toggleFlyout("weather", dead);
  expect(useSmabar.getState().openFlyout).toBeNull();

  useSmabar.getState().peekFlyout("weather", dead);
  expect(useSmabar.getState().openFlyout).toBeNull();

  // A valid rect still opens, and toggling a pinned flyout CLOSED keeps
  // working even when the trigger measured degenerate mid-collapse.
  useSmabar.getState().toggleFlyout("weather", rectA);
  expect(useSmabar.getState().openFlyout).toBe("weather");
  useSmabar.getState().toggleFlyout("weather", dead);
  expect(useSmabar.getState().openFlyout).toBeNull();
});

test("all flyout entry points reject anchors whose center left the viewport", () => {
  const retracted = { left: 10, top: 792, width: 40, height: 20 };

  useSmabar.getState().toggleFlyout("weather", retracted);
  expect(useSmabar.getState().openFlyout).toBeNull();

  useSmabar.getState().peekFlyout("weather", retracted, true);
  expect(useSmabar.getState().openFlyout).toBeNull();

  expect(useSmabar.getState().openPinnedFlyout("weather", retracted)).toBe(
    false,
  );
  expect(useSmabar.getState().openFlyout).toBeNull();
});

test("reordering closes flyouts and blocks every open path until the drag ends", () => {
  const store = useSmabar.getState();
  store.openPinnedFlyout("weather", rectA);
  store.setReordering(true);
  expect(useSmabar.getState()).toMatchObject({
    openFlyout: null,
    flyoutMode: null,
    triggerRect: null,
  });

  store.peekFlyout("weather", rectA, true);
  store.toggleFlyout("weather", rectA);
  expect(store.openPinnedFlyout("weather", rectA)).toBe(false);
  expect(useSmabar.getState().openFlyout).toBeNull();

  store.setReordering(false);
  expect(useSmabar.getState().openFlyout).toBeNull();
  store.peekFlyout("weather", rectA, true);
  expect(useSmabar.getState().flyoutMode).toBe("peek");
  expect(store.openPinnedFlyout("weather", rectA)).toBe(true);
});

test("openPinnedFlyout is a guarded idempotent open operation", () => {
  expect(useSmabar.getState().openPinnedFlyout("weather", rectA)).toBe(true);
  expect(useSmabar.getState().openPinnedFlyout("weather", rectB)).toBe(true);
  const state = useSmabar.getState();
  expect(state.openFlyout).toBe("weather");
  expect(state.flyoutMode).toBe("pinned");
  expect(state.triggerRect).toEqual(rectB);
});

test("clicking a peek converts it to a pinned flyout", () => {
  useSmabar.getState().peekFlyout("weather", rectA);
  expect(useSmabar.getState().flyoutMode).toBe("peek");

  useSmabar.getState().toggleFlyout("weather", rectB);
  expect(useSmabar.getState().openFlyout).toBe("weather");
  expect(useSmabar.getState().flyoutMode).toBe("pinned");
  expect(useSmabar.getState().triggerRect).toEqual(rectB);
});

test("a stale peek leave never closes another or pinned flyout", () => {
  useSmabar.getState().peekFlyout("weather", rectA);
  useSmabar.getState().peekFlyout("mail", rectB);
  useSmabar.getState().closePeek("weather");
  expect(useSmabar.getState().openFlyout).toBe("mail");

  useSmabar.getState().toggleFlyout("mail", rectB);
  useSmabar.getState().closePeek("mail");
  expect(useSmabar.getState().openFlyout).toBe("mail");
  expect(useSmabar.getState().flyoutMode).toBe("pinned");
});

test("a pending hover timer cannot open a peek after the effect is disabled", () => {
  const state = useSmabar.getState();
  state.setEffects({
    ...state.effects,
    hoverPeek: { ...state.effects.hoverPeek, enabled: false },
  });

  useSmabar.getState().peekFlyout("weather", rectA);
  expect(useSmabar.getState().openFlyout).toBeNull();
  expect(useSmabar.getState().flyoutMode).toBeNull();
});

test("forced peeks (tile hover content) override the disabled effect", () => {
  const state = useSmabar.getState();
  state.setEffects({
    ...state.effects,
    hoverPeek: { ...state.effects.hoverPeek, enabled: false },
  });

  useSmabar.getState().peekFlyout("weather", rectA, true);
  expect(useSmabar.getState().openFlyout).toBe("weather");
  expect(useSmabar.getState().flyoutMode).toBe("peek");

  // A pinned flyout still blocks even a forced peek.
  useSmabar.getState().toggleFlyout("weather", rectA);
  useSmabar.getState().peekFlyout("mail", rectB, true);
  expect(useSmabar.getState().openFlyout).toBe("weather");
  expect(useSmabar.getState().flyoutMode).toBe("pinned");
});

test("toggleFlyout with the same id closes the flyout", () => {
  useSmabar.getState().toggleFlyout("weather", rectA);
  useSmabar.getState().toggleFlyout("weather", rectB);
  expect(useSmabar.getState().openFlyout).toBeNull();
  expect(useSmabar.getState().triggerRect).toBeNull();
});

test("toggleFlyout with a different id replaces the open flyout", () => {
  useSmabar.getState().toggleFlyout("weather", rectA);
  useSmabar.getState().toggleFlyout("plugin:crypto:crypto", rectB);
  expect(useSmabar.getState().openFlyout).toBe("plugin:crypto:crypto");
  expect(useSmabar.getState().triggerRect).toEqual(rectB);
});

test("setLayout closes the open flyout and the solo overlay", () => {
  useSmabar.getState().toggleFlyout("clock", rectA);
  useSmabar.getState().setOverlayOpen(true);
  useSmabar.getState().setLayout({
    monitor: null,
    position: "top",
    variant: "rows",
    dividerRatio: 0.5,
    primaryZone: "shortcuts",
    width: "auto",
    margin: 12,
    maxWidth: 780,
    behavior: "float",
    yieldToFullscreen: false,
  });
  const state = useSmabar.getState();
  expect(state.layout.position).toBe("top");
  expect(state.layout.variant).toBe("rows");
  expect(state.layout.maxWidth).toBe(780);
  expect(state.layout.behavior).toBe("float");
  expect(state.layout.yieldToFullscreen).toBe(false);
  expect(state.openFlyout).toBeNull();
  expect(state.triggerRect).toBeNull();
  expect(state.flyoutMode).toBeNull();
  expect(state.overlayOpen).toBe(false);
});

test("setDividerRatio adjusts the ratio without closing surfaces", () => {
  useSmabar.getState().toggleFlyout("clock", rectA);
  useSmabar.getState().setDividerRatio(0.6);
  const state = useSmabar.getState();
  expect(state.layout.dividerRatio).toBe(0.6);
  expect(state.openFlyout).toBe("clock");
});

test("simple config fields follow their live events", () => {
  useSmabar.getState().setLanguage("de");
  useSmabar.getState().setZOrder("bottom");
  useSmabar
    .getState()
    .setSettingsWindow({ width: 960, height: 720, x: 40, y: null });
  expect(useSmabar.getState().language).toBe("de");
  expect(useSmabar.getState().zOrder).toBe("bottom");
  expect(useSmabar.getState().settingsWindow).toEqual({
    width: 960,
    height: 720,
    x: 40,
    y: null,
  });
});

test("bumpRegistryVersion increments the version", () => {
  useSmabar.getState().bumpRegistryVersion();
  useSmabar.getState().bumpRegistryVersion();
  expect(useSmabar.getState().registryVersion).toBe(2);
});

test("setPluginStatus keeps one status per plugin", () => {
  useSmabar.getState().setPluginStatus("a", { status: "running" });
  useSmabar
    .getState()
    .setPluginStatus("b", { status: "failed", error: "boom" });
  expect(useSmabar.getState().pluginStatus).toEqual({
    a: { status: "running" },
    b: { status: "failed", error: "boom" },
  });
});

test("setPluginUi stores html under its target key", () => {
  useSmabar.getState().setPluginUi("a/w/tile", "<b>x</b>");
  useSmabar.getState().setPluginUi("a/w/tile", "<b>y</b>");
  useSmabar.getState().setPluginUi("a/w/flyout", "<i>z</i>");
  expect(useSmabar.getState().pluginUi).toEqual({
    "a/w/tile": "<b>y</b>",
    "a/w/flyout": "<i>z</i>",
  });
});

test("setPluginUi ignores html that is already shown", () => {
  const listener = vi.fn();
  useSmabar.getState().setPluginUi("a/w/tile", "<b>x</b>");
  const unsubscribe = useSmabar.subscribe(listener);
  useSmabar.getState().setPluginUi("a/w/tile", "<b>x</b>");
  expect(listener).not.toHaveBeenCalled();
  useSmabar.getState().setPluginUi("a/w/tile", "<b>y</b>");
  expect(listener).toHaveBeenCalledTimes(1);
  unsubscribe();
});

test("dropPluginUi forgets only the listed tiles of that plugin", () => {
  const store = useSmabar.getState();
  store.setPluginUi("ai/claude/tile", "<b>gone</b>");
  store.setPluginUi("ai/claude/flyout", "<b>gone too</b>");
  store.setPluginUi("ai/usage/tile", "<b>stays</b>");
  store.setPluginUi("weather/claude/tile", "<b>other plugin</b>");

  store.dropPluginUi("ai", ["claude"]);

  expect(useSmabar.getState().pluginUi).toEqual({
    "ai/usage/tile": "<b>stays</b>",
    "weather/claude/tile": "<b>other plugin</b>",
  });
});

test("dropPluginUi is a no-op for tiles that have no cached html", () => {
  useSmabar.getState().setPluginUi("ai/usage/tile", "<b>x</b>");
  useSmabar.getState().dropPluginUi("ai", ["nothing-here"]);
  expect(useSmabar.getState().pluginUi).toEqual({
    "ai/usage/tile": "<b>x</b>",
  });
});

test("disabled popups discard incoming renders and clear the visible stack", () => {
  const popup = { pluginId: "mail", tileId: "main", html: "<p>Hi</p>" };
  useSmabar.getState().enqueuePopup(popup, 1_000);
  useSmabar.getState().enqueuePopup(popup, 1_001);
  expect(useSmabar.getState().popupQueue.visible).toHaveLength(2);

  useSmabar.getState().setPopups({ enabled: false, position: "bottom-right" });
  useSmabar.getState().enqueuePopup(popup, 40_000);
  expect(useSmabar.getState().popupQueue.visible).toEqual([]);
  expect(useSmabar.getState().popupQueue.queued).toEqual([]);
});

test("dismissPopup removes exactly the addressed toast", () => {
  const popup = { pluginId: "mail", tileId: "main", html: "<p>Hi</p>" };
  useSmabar.getState().enqueuePopup(popup, 1_000);
  useSmabar.getState().enqueuePopup(popup, 1_001);
  const [first] = useSmabar.getState().popupQueue.visible;
  expect(first).toBeDefined();
  if (!first) return;
  useSmabar.getState().dismissPopup(first.id, 2_000);
  const remaining = useSmabar.getState().popupQueue.visible;
  expect(remaining).toHaveLength(1);
  expect(remaining[0]?.id).not.toBe(first.id);
});

test("setRuntimeStatus replaces the runtime snapshot", () => {
  expect(useSmabar.getState().runtimeStatus).toBeNull();
  useSmabar.getState().setRuntimeStatus({ state: "installing", detail: "dl" });
  expect(useSmabar.getState().runtimeStatus).toEqual({
    state: "installing",
    detail: "dl",
  });
  useSmabar.getState().setRuntimeStatus({ state: "ready" });
  expect(useSmabar.getState().runtimeStatus?.state).toBe("ready");
});
