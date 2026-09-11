// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { setLocale } from "../../i18n/t";
import { useSmabar, type ThemeSummary } from "../../store/bar";
import { ColorSettings } from "./ColorSettings";

const FONTS = {
  sans: { family: "system-ui", source: "system" },
  mono: { family: "monospace", source: "system" },
};

const THEMES: ThemeSummary[] = [
  {
    name: "default",
    source: "bundled",
    active: true,
    colors: {
      accent: "#8b5cf6",
      accent2: "#ec4899",
      surface: "#141626",
      text: "#ffffff",
    },
    fonts: FONTS,
  },
  {
    name: "carbon",
    source: "bundled",
    active: false,
    colors: {
      accent: "#f59e0b",
      accent2: "#fcd34d",
      surface: "#141519",
      text: "#eceef1",
    },
    fonts: FONTS,
  },
];

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.useFakeTimers();
  (
    globalThis as typeof globalThis & {
      IS_REACT_ACT_ENVIRONMENT?: boolean;
    }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  setLocale({});
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  const store = useSmabar.getState();
  store.setTheme("default");
  store.setAppearance({ ...store.appearance, tokens: {} });
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  vi.clearAllTimers();
  vi.useRealTimers();
  (
    globalThis as typeof globalThis & {
      IS_REACT_ACT_ENVIRONMENT?: boolean;
    }
  ).IS_REACT_ACT_ENVIRONMENT = false;
});

function render(tokens: Record<string, string> = {}, themes = THEMES): void {
  const store = useSmabar.getState();
  act(() => {
    store.setAppearance({ ...store.appearance, tokens });
    root.render(<ColorSettings themes={themes} />);
  });
}

function controls(): HTMLElement[] {
  return [
    ...container.querySelectorAll<HTMLElement>(".settings-color-control"),
  ];
}

function option(control: HTMLElement, label: string): HTMLButtonElement {
  const button = [
    ...control.querySelectorAll<HTMLButtonElement>("button"),
  ].find((candidate) => candidate.getAttribute("aria-label") === label);
  if (button === undefined) throw new Error(`missing option: ${label}`);
  return button;
}

function setInput(input: HTMLInputElement, value: string): void {
  act(() => {
    Reflect.set(HTMLInputElement.prototype, "value", value, input);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function controlAt(index: number): HTMLElement {
  const control = controls()[index];
  if (control === undefined) {
    throw new Error(`missing colour control ${String(index)}`);
  }
  return control;
}

test("every row exposes exactly one selected source", () => {
  render();
  for (const control of controls()) {
    expect(control.querySelectorAll('[aria-pressed="true"]')).toHaveLength(1);
  }

  render({
    "--sb-accent": "#f59e0b",
    "--sb-accent-2": "#123456",
    "--sb-bar-bg": "#141519",
    "--sb-text": "#abcdef",
  });
  for (const control of controls()) {
    expect(control.querySelectorAll('[aria-pressed="true"]')).toHaveLength(1);
  }
});

test("theme sources are named while automatic text shows its derived value", () => {
  render({ "--sb-bar-bg": "#faf8f5" });
  const rows = controls();
  expect(
    rows[0]?.querySelector(".settings-color-source")?.textContent,
  ).toContain("From the theme");
  expect(
    rows[0]?.querySelector(".settings-color-source")?.textContent,
  ).toContain("Default");
  expect(rows[3]?.querySelector(".settings-color-source")?.textContent).toBe(
    "Automatic",
  );
  expect(rows[3]?.querySelector(".settings-color-value")?.textContent).toBe(
    "#09060f",
  );
  expect(
    rows[3]?.querySelector<HTMLElement>(".settings-color-preview")?.style
      .background,
  ).toBe("#09060f");
});

test("editing either accent never creates an override for its sibling", () => {
  render();
  act(() => {
    option(controlAt(0), "Accent color: Carbon #f59e0b").click();
  });
  expect(useSmabar.getState().appearance.tokens).toMatchObject({
    "--sb-accent": "#f59e0b",
    "--sb-accent-gradient":
      "linear-gradient(135deg, var(--sb-accent), var(--sb-accent-2))",
  });
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent-2",
  );

  render();
  act(() => {
    option(controlAt(1), "Second accent color: Carbon #fcd34d").click();
  });
  expect(useSmabar.getState().appearance.tokens).toMatchObject({
    "--sb-accent-2": "#fcd34d",
  });
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent",
  );
});

test("resetting one accent preserves the other and its shared gradient", () => {
  render({
    "--sb-accent": "#112233",
    "--sb-accent-2": "#ddeeff",
    "--sb-accent-gradient": "custom-gradient",
    "--sb-accent-glow": "custom-glow",
  });
  act(() => {
    const auto = controls()[0]?.querySelector<HTMLButtonElement>("[data-auto]");
    if (auto === null || auto === undefined) throw new Error("missing reset");
    auto.click();
  });
  expect(useSmabar.getState().appearance.tokens).toMatchObject({
    "--sb-accent-2": "#ddeeff",
    "--sb-accent-gradient": "custom-gradient",
  });
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent",
  );
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent-glow",
  );

  act(() => {
    const auto = controls()[1]?.querySelector<HTMLButtonElement>("[data-auto]");
    if (auto === null || auto === undefined) throw new Error("missing reset");
    auto.click();
  });
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent-2",
  );
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent-gradient",
  );

  render({
    "--sb-accent": "#112233",
    "--sb-accent-2": "#ddeeff",
    "--sb-accent-gradient": "custom-gradient",
    "--sb-accent-glow": "custom-glow",
  });
  act(() => {
    const auto = controlAt(1).querySelector<HTMLButtonElement>("[data-auto]");
    if (auto === null) throw new Error("missing reset");
    auto.click();
  });
  expect(useSmabar.getState().appearance.tokens).toMatchObject({
    "--sb-accent": "#112233",
    "--sb-accent-gradient": "custom-gradient",
    "--sb-accent-glow": "custom-glow",
  });
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent-2",
  );
});

test("non-hex theme colours stay visible without a black native fallback", () => {
  const modern = structuredClone(THEMES);
  const primary = modern[0];
  if (primary === undefined) throw new Error("missing primary theme");
  primary.colors.accent = "oklch(76.5% 0.177 163.223)";
  render({}, modern);
  const accent = controlAt(0);
  expect(accent.querySelector(".settings-color-value")?.textContent).toBe(
    "oklch(76.5% 0.177 163.223)",
  );
  expect(
    accent.querySelector<HTMLInputElement>('input[type="color"]')?.value,
  ).toBe("#ffffff");
});

test("invalid hex stays local while a complete value updates the colour", () => {
  render();
  const hex = controls()[0]?.querySelector<HTMLInputElement>(
    ".settings-color-hex input",
  );
  if (hex === null || hex === undefined) throw new Error("missing hex input");

  setInput(hex, "#12zzzz");
  act(() => {
    hex.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  });
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent",
  );
  expect(hex.getAttribute("aria-invalid")).toBe("true");

  setInput(hex, "#112233");
  expect(useSmabar.getState().appearance.tokens["--sb-accent"]).toBe("#112233");
});

test("toolbar arrows move focus while Enter and Space activate choices", () => {
  render();
  const accent = controlAt(0);
  const automatic = accent.querySelector<HTMLButtonElement>("[data-auto]");
  const carbon = option(accent, "Accent color: Carbon #f59e0b");
  if (automatic === null) throw new Error("missing automatic option");
  automatic.focus();

  act(() => {
    automatic.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }),
    );
  });
  expect(document.activeElement).toBe(carbon);
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent",
  );

  act(() => {
    carbon.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
    );
  });
  expect(useSmabar.getState().appearance.tokens["--sb-accent"]).toBe("#f59e0b");

  const reset = controls()[0]?.querySelector<HTMLButtonElement>("[data-auto]");
  if (reset === null || reset === undefined) throw new Error("missing reset");
  act(() => {
    reset.dispatchEvent(
      new KeyboardEvent("keydown", { key: " ", bubbles: true }),
    );
  });
  expect(useSmabar.getState().appearance.tokens).not.toHaveProperty(
    "--sb-accent",
  );
});

test("low contrast is explained for accent pairs and explicit text", () => {
  render({
    "--sb-accent": "#141626",
    "--sb-accent-2": "#fcd34d",
    "--sb-bar-bg": "#faf8f5",
    "--sb-text": "#ffffff",
  });
  expect(
    controls()[1]?.querySelector(".settings-color-warning")?.textContent,
  ).toBe("No single foreground reaches 4.5:1 on both accent colors.");
  expect(
    controls()[3]?.querySelector(".settings-color-warning")?.textContent,
  ).toBe("This text color is below the recommended 4.5:1 contrast.");
});
