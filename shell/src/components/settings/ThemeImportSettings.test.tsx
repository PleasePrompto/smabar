// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { setLocale } from "../../i18n/t";
import { useSmabar, type ThemeSummary } from "../../store/bar";
import { ThemeImportSettings } from "./ThemeImportSettings";
import {
  BUNDLED,
  createThemeManagerTestHarness,
  DROPIN,
  flush,
  FRESH_LIST,
  typeInput,
  type ThemeManagerTestHarness,
} from "./ThemeManager.testHarness";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../../ipc/call", () => ({ call: callMock }));

let harness: ThemeManagerTestHarness;

beforeEach(() => {
  harness = createThemeManagerTestHarness();
  callMock.mockReset();
  callMock.mockResolvedValue(null);
});

afterEach(() => {
  harness.dispose();
});

async function render(themes: ThemeSummary[]): Promise<void> {
  await harness.render(
    <ThemeImportSettings themes={themes} onThemes={harness.onThemes} />,
  );
}

test("importing a colliding file asks before overwriting", async () => {
  callMock.mockImplementation((command: string) =>
    Promise.resolve(command === "import_theme" ? FRESH_LIST : null),
  );
  await render([BUNDLED, DROPIN]);
  await flush(() => {
    typeInput(harness.input("Theme file"), "/downloads/Mine.json");
  });
  await flush(() => {
    harness.button("Import").click();
  });
  expect(callMock).not.toHaveBeenCalledWith("import_theme", expect.anything());
  await flush(() => {
    harness.confirmAction().click();
  });
  expect(callMock).toHaveBeenCalledWith("import_theme", {
    path: "/downloads/Mine.json",
    overwrite: true,
  });
  expect(useSmabar.getState().notice).toBe("settings.themes.imported");
});

test("a theme file dropped onto the settings runs the import flow", async () => {
  callMock.mockImplementation((command: string) =>
    Promise.resolve(command === "import_theme" ? FRESH_LIST : null),
  );
  await render([BUNDLED]);
  await flush(() => {
    useSmabar.getState().setThemeImportPath("/downloads/fresh.json");
  });
  expect(callMock).toHaveBeenCalledWith("import_theme", {
    path: "/downloads/fresh.json",
    overwrite: false,
  });
  expect(useSmabar.getState().themeImportPath).toBeNull();
  expect(harness.onThemes).toHaveBeenCalledWith(FRESH_LIST);
});

test("an invalid theme file shows the core's error inline", async () => {
  callMock.mockImplementation((command: string) =>
    command === "import_theme"
      ? Promise.reject(new Error("invalid theme document: not a JSON object"))
      : Promise.resolve(null),
  );
  await render([BUNDLED]);
  await flush(() => {
    typeInput(harness.input("Theme file"), "/downloads/broken.json");
  });
  await flush(() => {
    harness.button("Import").click();
  });
  const alert = harness.container.querySelector('[role="alert"]');
  expect(alert?.textContent).toContain("invalid theme document");
  expect(harness.onThemes).not.toHaveBeenCalled();
  expect(callMock).toHaveBeenCalledWith(
    "ui_log",
    expect.objectContaining({ level: "error" }),
  );
});

test("a confirmed import failure stays visible, logged, and returns focus", async () => {
  callMock.mockImplementation((command: string) =>
    command === "import_theme"
      ? Promise.reject(new Error("cannot import theme"))
      : Promise.resolve(null),
  );
  await render([BUNDLED, DROPIN]);
  await flush(() => {
    typeInput(harness.input("Theme file"), "/downloads/Mine.json");
  });
  await flush(() => {
    harness.button("Import").click();
  });
  await flush(() => {
    harness.confirmAction().click();
  });
  await flush(() => {
    vi.runOnlyPendingTimers();
  });
  expect(harness.container.textContent).toContain("cannot import theme");
  expect(document.activeElement).toBe(harness.button("Import"));
  expect(
    callMock.mock.calls.filter(([command]) => command === "ui_log"),
  ).toHaveLength(1);
});

test("the import placeholder is translated", async () => {
  setLocale({
    "settings.themes.importPlaceholder": "~/Downloads/mein-design.json",
  });
  await render([BUNDLED]);
  expect(harness.input("Theme file").placeholder).toBe(
    "~/Downloads/mein-design.json",
  );
});
