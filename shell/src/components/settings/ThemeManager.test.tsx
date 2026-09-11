// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar, type ThemeSummary } from "../../store/bar";
import { ThemeManager } from "./ThemeManager";
import {
  BUNDLED,
  createThemeManagerTestHarness,
  DROPIN,
  flush,
  FRESH_LIST,
  typeInput,
  type ThemeManagerTestHarness,
} from "./ThemeManager.testHarness";
import { themeDisplayName } from "./model";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../../ipc/call", () => ({ call: callMock }));
let harness: ThemeManagerTestHarness;

beforeEach(() => {
  harness = createThemeManagerTestHarness();
  callMock.mockReset();
  callMock.mockImplementation((command: string) =>
    command === "get_theme_export_dir"
      ? Promise.resolve({ configured: "", effective: "/x/export" })
      : Promise.resolve(null),
  );
});

afterEach(() => {
  harness.dispose();
});

async function render(themes: ThemeSummary[]): Promise<void> {
  await harness.render(
    <ThemeManager themes={themes} onThemes={harness.onThemes} />,
  );
}

test("saving slugifies the typed name and reports the fresh list", async () => {
  callMock.mockImplementation((command: string) =>
    Promise.resolve(command === "save_custom_theme" ? FRESH_LIST : null),
  );
  await render([BUNDLED]);
  await flush(() => {
    typeInput(harness.input("Save current look"), "My Look (1)");
  });
  await flush(() => {
    harness.button("Save").click();
  });
  expect(callMock).toHaveBeenCalledWith("save_custom_theme", {
    name: "my-look-1",
    overwrite: false,
  });
  expect(harness.onThemes).toHaveBeenCalledWith(FRESH_LIST);
  expect(useSmabar.getState().notice).toBe("settings.themes.saved");
  expect(harness.input("Save current look").value).toBe("");
});

test("saving over an existing drop-in asks first", async () => {
  await render([BUNDLED, DROPIN]);
  await flush(() => {
    typeInput(harness.input("Save current look"), "Mine");
  });
  const save = harness.button("Save");
  await flush(() => {
    save.click();
  });
  expect(callMock).not.toHaveBeenCalledWith(
    "save_custom_theme",
    expect.anything(),
  );
  expect(document.activeElement).toBe(harness.button("Cancel"));
  const confirm =
    harness.container.querySelector<HTMLElement>("[data-confirm-row]");
  const question = confirm?.querySelector<HTMLElement>("[id]");
  expect(confirm?.getAttribute("aria-describedby")).toBe(question?.id);
  await flush(() => {
    harness.button("Cancel").click();
  });
  expect(document.activeElement).toBe(save);
  await flush(() => {
    save.click();
  });
  await flush(() => {
    harness.confirmAction().click();
  });
  expect(callMock).toHaveBeenCalledWith("save_custom_theme", {
    name: "mine",
    overwrite: true,
  });
});

test("a bundled name blocks saving with a hint", async () => {
  await render([BUNDLED]);
  await flush(() => {
    typeInput(harness.input("Save current look"), "Default");
  });
  const save = harness.button("Save");
  const field = harness.input("Save current look");
  const hintId = field.getAttribute("aria-describedby");
  expect(save.disabled).toBe(true);
  expect(field.getAttribute("aria-invalid")).toBe("true");
  expect(hintId).not.toBeNull();
  expect(document.getElementById(hintId ?? "")?.getAttribute("role")).toBe(
    "alert",
  );
  expect(document.getElementById(hintId ?? "")?.textContent).toContain(
    "This name belongs to a built-in theme",
  );
});

test("custom themes show their display name and delete after a confirm", async () => {
  callMock.mockImplementation((command: string) =>
    Promise.resolve(command === "delete_theme" ? FRESH_LIST : null),
  );
  await render([BUNDLED, DROPIN]);
  expect(themeDisplayName(DROPIN)).toBe("My Look");
  expect(harness.container.textContent).toContain("My Look");
  const remove = harness.container.querySelector<HTMLButtonElement>(
    '.sb-list button[aria-label="Delete"]',
  );
  if (remove === null) throw new Error("no delete button");
  await flush(() => {
    remove.click();
  });
  expect(callMock).not.toHaveBeenCalledWith("delete_theme", expect.anything());
  await flush(() => {
    harness.confirmAction().click();
  });
  expect(callMock).toHaveBeenCalledWith("delete_theme", { name: "mine" });
  expect(harness.onThemes).toHaveBeenCalledWith(FRESH_LIST);
  expect(useSmabar.getState().notice).toBe("settings.themes.deleted");
});

test("confirmed action failures stay visible, logged, and return focus", async () => {
  callMock.mockImplementation((command: string) => {
    if (command === "get_theme_export_dir") {
      return Promise.resolve({ configured: "", effective: "/x/export" });
    }
    if (command === "save_custom_theme") {
      return Promise.reject(new Error("disk is read-only"));
    }
    if (command === "delete_theme") {
      return Promise.reject(new Error("cannot delete theme"));
    }
    if (command === "export_theme") {
      return Promise.reject(new Error("export directory denied"));
    }
    return Promise.resolve(null);
  });
  await render([BUNDLED, DROPIN]);
  await flush(() => {
    typeInput(harness.input("Save current look"), "Mine");
  });
  await flush(() => {
    harness.button("Save").click();
  });
  await flush(() => {
    harness.confirmAction().click();
  });
  await flush(() => {
    vi.runOnlyPendingTimers();
  });
  expect(
    harness.container.querySelector('[role="alert"]')?.textContent,
  ).toContain("disk is read-only");
  expect(document.activeElement).toBe(harness.button("Save"));

  const remove = harness.container.querySelector<HTMLButtonElement>(
    '.sb-list button[aria-label="Delete"]',
  );
  if (remove === null) throw new Error("no delete button");
  await flush(() => {
    remove.click();
  });
  await flush(() => {
    harness.confirmAction().click();
  });
  await flush(() => {
    vi.runOnlyPendingTimers();
  });
  expect(
    harness.container.querySelector('[role="alert"]')?.textContent,
  ).toContain("cannot delete theme");
  expect(document.activeElement).toBe(
    harness.container.querySelector('.sb-list button[aria-label="Delete"]'),
  );

  await flush(() => {
    harness.button("Export active theme").click();
  });
  await flush(() => {
    vi.runOnlyPendingTimers();
  });
  expect(harness.container.textContent).toContain("export directory denied");
  expect(document.activeElement).toBe(harness.button("Export active theme"));
  expect(
    callMock.mock.calls.filter(([command]) => command === "ui_log"),
  ).toHaveLength(3);
});

test("exporting the active theme reports the written path", async () => {
  callMock.mockImplementation((command: string) => {
    if (command === "get_theme_export_dir") {
      return Promise.resolve({ configured: "", effective: "/x/export" });
    }
    return Promise.resolve(
      command === "export_theme" ? "/x/export/default.json" : null,
    );
  });
  await render([BUNDLED]);
  await flush(() => {
    harness.button("Export active theme").click();
  });
  expect(callMock).toHaveBeenCalledWith("export_theme", {
    name: "default",
    directory: "",
  });
  expect(harness.container.textContent).toContain("/x/export/default.json");
  expect(useSmabar.getState().notice).toBe("settings.themes.exported");
});

test("an invalid configured export path remains editable", async () => {
  callMock.mockImplementation((command: string) =>
    command === "get_theme_export_dir"
      ? Promise.resolve({
          configured: "relative/path",
          effective: null,
          error: 'themeExportDir: "relative/path" is not an absolute path',
        })
      : Promise.resolve(null),
  );
  await render([BUNDLED]);
  expect(harness.input("Export folder").value).toBe("relative/path");
  expect(
    harness.container.querySelector('[role="alert"]')?.textContent,
  ).toContain("not an absolute path");
});

test("a failed export-folder read leaves an editable recovery field", async () => {
  let rejectRead: (error: Error) => void = () => undefined;
  const loading = new Promise<never>((_resolve, reject) => {
    rejectRead = reject;
  });
  callMock.mockImplementation((command: string) =>
    command === "get_theme_export_dir" ? loading : Promise.resolve(null),
  );
  await render([BUNDLED]);
  expect(harness.input("Export folder").disabled).toBe(true);
  await flush(() => {
    rejectRead(new Error("cannot read export folder"));
  });
  expect(harness.container.textContent).toContain("cannot read export folder");
  expect(harness.input("Export folder").disabled).toBe(false);

  await flush(() => {
    typeInput(harness.input("Export folder"), "/recovered");
  });
  expect(harness.input("Export folder").value).toBe("/recovered");
  await flush(() => {
    vi.advanceTimersByTime(300);
  });
  expect(callMock).toHaveBeenCalledWith("update_config", {
    path: "themeExportDir",
    value: "/recovered",
  });
});

test("export uses the field value immediately without waiting for persistence", async () => {
  callMock.mockImplementation((command: string) => {
    if (command === "get_theme_export_dir") {
      return Promise.resolve({ configured: "", effective: "/x/export" });
    }
    return Promise.resolve(
      command === "export_theme" ? "/fresh/default.json" : null,
    );
  });
  await render([BUNDLED]);
  await flush(() => {
    typeInput(harness.input("Export folder"), "/fresh");
  });
  await flush(() => {
    harness.button("Export active theme").click();
  });
  expect(callMock).toHaveBeenCalledWith("export_theme", {
    name: "default",
    directory: "/fresh",
  });
});

test("a pending export ignores double-submit and disables export buttons", async () => {
  let resolveExport: (path: string) => void = () => undefined;
  const pending = new Promise<string>((resolve) => {
    resolveExport = resolve;
  });
  callMock.mockImplementation((command: string) => {
    if (command === "get_theme_export_dir") {
      return Promise.resolve({ configured: "", effective: "/x/export" });
    }
    return command === "export_theme" ? pending : Promise.resolve(null);
  });
  await render([BUNDLED, DROPIN]);
  const exportActive = harness.button("Export active theme");
  await flush(() => {
    exportActive.click();
    exportActive.click();
  });
  expect(
    callMock.mock.calls.filter(([command]) => command === "export_theme"),
  ).toHaveLength(1);
  expect(exportActive.disabled).toBe(true);

  await flush(() => {
    resolveExport("/x/export/default.json");
  });
  expect(exportActive.disabled).toBe(false);
});
