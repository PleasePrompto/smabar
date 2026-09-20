import { act, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { vi } from "vitest";

import { setLocale } from "../../i18n/t";
import { resetUiLog } from "../../ipc/log";
import { useSmabar, type ThemeSummary } from "../../store/bar";

const originalReportError = globalThis.reportError;

const FONTS = {
  sans: { family: "system-ui", source: "system" },
  mono: { family: "monospace", source: "system" },
};
const COLORS = {
  accent: "#8b5cf6",
  accent2: "#ec4899",
  surface: "#141626",
  text: "#ffffff",
};

function summary(
  name: string,
  source: ThemeSummary["source"],
  extra: Partial<ThemeSummary> = {},
): ThemeSummary {
  return {
    name,
    source,
    active: name === "default",
    colors: COLORS,
    fonts: FONTS,
    preview: {
      layout: useSmabar.getInitialState().layout,
      appearance: useSmabar.getInitialState().appearance,
    },
    ...extra,
  };
}

export const BUNDLED = summary("default", "bundled");
export const DROPIN = summary("mine", "dropin", {
  meta: { name: "My Look" },
});
export const FRESH_LIST = [BUNDLED, summary("new-one", "dropin")];

export interface ThemeManagerTestHarness {
  container: HTMLDivElement;
  onThemes: ReturnType<typeof vi.fn<(themes: ThemeSummary[]) => void>>;
  render: (node: ReactNode) => Promise<void>;
  input: (label: string) => HTMLInputElement;
  button: (text: string) => HTMLButtonElement;
  confirmAction: () => HTMLButtonElement;
  dispose: () => void;
}

export async function flush(action: () => void): Promise<void> {
  await act(async () => {
    action();
    await Promise.resolve();
  });
}

export function typeInput(field: HTMLInputElement, value: string): void {
  Reflect.set(HTMLInputElement.prototype, "value", value, field);
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

export function createThemeManagerTestHarness(): ThemeManagerTestHarness {
  vi.useFakeTimers();
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  setLocale({});
  resetUiLog();
  globalThis.reportError = vi.fn();

  const container = document.createElement("div");
  document.body.append(container);
  const root: Root = createRoot(container);
  const onThemes = vi.fn<(themes: ThemeSummary[]) => void>();
  const store = useSmabar.getState();
  store.setTheme("default");
  store.setNotice(null);
  store.setThemeImportPath(null);
  store.setSettingsGroup("bar");

  return {
    container,
    onThemes,
    render: (node) =>
      flush(() => {
        root.render(node);
      }),
    input: (label) => {
      const field = container.querySelector<HTMLInputElement>(
        `input[aria-label="${label}"]`,
      );
      if (field === null) throw new Error(`no input "${label}"`);
      return field;
    },
    button: (text) => {
      const match = [...container.querySelectorAll("button")].find(
        (candidate) => candidate.textContent === text,
      );
      if (match === undefined) throw new Error(`no button "${text}"`);
      return match;
    },
    confirmAction: () => {
      const danger = container.querySelector<HTMLButtonElement>(
        "[data-confirm-row] .sb-btn-danger",
      );
      if (danger === null) throw new Error("no confirm dialog");
      return danger;
    },
    dispose: () => {
      act(() => {
        root.unmount();
      });
      container.remove();
      vi.clearAllTimers();
      vi.useRealTimers();
      globalThis.reportError = originalReportError;
      (
        globalThis as typeof globalThis & {
          IS_REACT_ACT_ENVIRONMENT?: boolean;
        }
      ).IS_REACT_ACT_ENVIRONMENT = false;
    },
  };
}
