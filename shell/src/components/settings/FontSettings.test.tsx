// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar, type ThemeSummary } from "../../store/bar";
import type { EnsuredGoogleFont, FontOption } from "../../theme/fonts";
import { FontSettings } from "./FontSettings";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../../ipc/call", () => ({ call: callMock }));

const SYSTEM: FontOption[] = [
  {
    id: "system:system-ui",
    family: "system-ui",
    source: "system",
    category: "sans-serif",
    monospaced: false,
    cached: false,
  },
  {
    id: "system:DejaVu Sans",
    family: "DejaVu Sans",
    source: "system",
    category: "sans-serif",
    monospaced: false,
    cached: false,
  },
  {
    id: "system:DejaVu Sans Mono",
    family: "DejaVu Sans Mono",
    source: "system",
    category: "monospace",
    monospaced: true,
    cached: false,
  },
];
const GOOGLE: FontOption = {
  id: "jetbrains-mono",
  family: "JetBrains Mono",
  source: "google",
  category: "monospace",
  monospaced: true,
  cached: false,
};
const GOOGLE_SANS: FontOption = {
  id: "noto-sans",
  family: "Noto Sans",
  source: "google",
  category: "sans-serif",
  monospaced: false,
  cached: false,
};
const THEMES: ThemeSummary[] = [
  {
    name: "default",
    source: "bundled",
    active: true,
    preview: {
      layout: useSmabar.getInitialState().layout,
      appearance: useSmabar.getInitialState().appearance,
    },
    colors: {
      accent: "#8b5cf6",
      accent2: "#ec4899",
      surface: "#141626",
      text: "#ffffff",
    },
    fonts: {
      sans: { family: "system-ui", source: "system" },
      mono: { family: "ui-monospace, monospace", source: "system" },
    },
  },
];

let container: HTMLDivElement;
let root: Root;
let resolveGoogle: ((font: EnsuredGoogleFont) => void) | undefined;

beforeEach(() => {
  vi.useFakeTimers();
  useSmabar.setState(useSmabar.getInitialState(), true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  callMock.mockImplementation(
    (command: string, args?: Record<string, unknown>) => {
      if (command === "font_list") {
        const fonts =
          args?.source === "google" ? [GOOGLE, GOOGLE_SANS] : SYSTEM;
        return Promise.resolve(
          args?.monospaced === true
            ? fonts.filter((font) => font.monospaced)
            : fonts,
        );
      }
      if (command === "ensure_google_font") {
        return new Promise<EnsuredGoogleFont>((resolve) => {
          resolveGoogle = resolve;
        });
      }
      return Promise.resolve(null);
    },
  );
});

afterEach(async () => {
  act(() => {
    root.unmount();
  });
  await vi.runOnlyPendingTimersAsync();
  container.remove();
  callMock.mockReset();
  resolveGoogle = undefined;
  vi.useRealTimers();
});

async function render(): Promise<void> {
  await act(async () => {
    root.render(<FontSettings themes={THEMES} />);
    await Promise.resolve();
  });
}

async function click(button: HTMLButtonElement): Promise<void> {
  await act(async () => {
    button.click();
    await Promise.resolve();
  });
}

function buttonNamed(name: string): HTMLButtonElement {
  const button = [
    ...container.querySelectorAll<HTMLButtonElement>("button"),
  ].find((candidate) => candidate.textContent.includes(name));
  if (button === undefined) throw new Error(`no button containing "${name}"`);
  return button;
}

test("UI and mono fonts are selected independently", async () => {
  await render();
  const pickers = container.querySelectorAll<HTMLButtonElement>(
    ".settings-font-current",
  );
  const uiPicker = pickers[0];
  if (uiPicker === undefined) throw new Error("UI font picker is missing");
  await click(uiPicker);
  await click(buttonNamed("DejaVu Sans"));

  expect(useSmabar.getState().appearance.tokens).toMatchObject({
    "--sb-font-sans": '"DejaVu Sans", sans-serif',
    "--sb-font-sans-source": "system",
  });
  expect(
    useSmabar.getState().appearance.tokens["--sb-font-mono"],
  ).toBeUndefined();
});

test("a late Google download cannot replace a newer system choice", async () => {
  await render();
  const monoPicker = container.querySelectorAll<HTMLButtonElement>(
    ".settings-font-current",
  )[1];
  if (monoPicker === undefined) throw new Error("mono font picker is missing");
  await click(monoPicker);
  await click(buttonNamed("Google Fonts"));
  await click(buttonNamed("JetBrains Mono"));
  expect(
    useSmabar.getState().appearance.tokens["--sb-font-mono-source"],
  ).toBeUndefined();

  await click(buttonNamed("System"));
  await click(buttonNamed("DejaVu Sans Mono"));
  const resolve = resolveGoogle;
  if (resolve === undefined) throw new Error("Google request was not started");
  await act(async () => {
    resolve({ id: GOOGLE.id, family: GOOGLE.family, faces: [] });
    await Promise.resolve();
  });

  expect(useSmabar.getState().appearance.tokens).toMatchObject({
    "--sb-font-mono": '"DejaVu Sans Mono", monospace',
    "--sb-font-mono-source": "system",
  });
});

test("a system family from another device keeps its fallback and says so", async () => {
  const base = THEMES[0];
  if (base === undefined) throw new Error("default theme is missing");
  const foreign: ThemeSummary = {
    ...base,
    fonts: {
      ...base.fonts,
      sans: {
        family: '"Made on another OS", sans-serif',
        source: "system",
      },
    },
  };
  await act(async () => {
    root.render(<FontSettings themes={[foreign]} />);
    await Promise.resolve();
  });
  await vi.waitFor(() => {
    expect(container.textContent).toContain(
      "Not available on this device · fallback active",
    );
  });
});

test("a CSS generic does not pretend to be a missing installed font", async () => {
  await render();
  expect(container.textContent).not.toContain(
    "Not available on this device · fallback active",
  );
});

test("a theme switch cancels an unfinished Google selection", async () => {
  await render();
  const uiPicker = container.querySelector<HTMLButtonElement>(
    ".settings-font-current",
  );
  if (uiPicker === null) throw new Error("UI font picker is missing");
  await click(uiPicker);
  await click(buttonNamed("Google Fonts"));
  await click(buttonNamed("Noto Sans"));

  act(() => {
    useSmabar.getState().setTheme("other-theme");
  });
  const resolve = resolveGoogle;
  if (resolve === undefined) throw new Error("Google request was not started");
  await act(async () => {
    resolve({ id: GOOGLE_SANS.id, family: GOOGLE_SANS.family, faces: [] });
    await Promise.resolve();
  });

  expect(
    useSmabar.getState().appearance.tokens["--sb-font-sans-source"],
  ).toBeUndefined();
});
