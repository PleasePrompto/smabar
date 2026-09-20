// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { ThemeManager } from "./ThemeManager";
import { setConfigDebounced } from "./persist";
import {
  BUNDLED,
  DROPIN,
  FRESH_LIST,
  createThemeManagerTestHarness,
  flush,
  typeInput,
  type ThemeManagerTestHarness,
} from "./ThemeManager.testHarness";
const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../../ipc/call", () => ({ call: callMock }));
let harness: ThemeManagerTestHarness;
const document = '{"--sb-accent":"#123456"}';
beforeEach(async () => {
  harness = createThemeManagerTestHarness();
  callMock.mockReset();
  callMock.mockImplementation((command: string) =>
    Promise.resolve(
      command === "choose_settings_file"
        ? "/downloads/new-one.json"
        : command === "save_theme_copy"
          ? {
              themes: FRESH_LIST,
              document,
              path: "/downloads/new-one.json",
              fileError: null,
            }
          : command === "delete_theme"
            ? [BUNDLED]
            : command === "theme_file_document"
              ? document
              : null,
    ),
  );
  await harness.render(
    <ThemeManager themes={[BUNDLED, DROPIN]} onThemes={harness.onThemes} />,
  );
});
afterEach(() => {
  harness.dispose();
});
async function name(value: string) {
  await flush(() => {
    typeInput(harness.input("Theme name"), value);
  });
}
async function click(label: string) {
  await flush(() => {
    harness.button(label).click();
  });
}
test("one save persists the latest slider edit before capturing and saving both copies", async () => {
  await name("New One");
  setConfigDebounced("appearance.tokens", { "--sb-accent": "#123456" });
  await click("Save theme…");
  expect(callMock).toHaveBeenCalledWith("save_theme_copy", {
    name: "new-one",
    overwrite: false,
    path: "/downloads/new-one.json",
  });
  const commands = callMock.mock.calls.map(([command]) => String(command));
  expect(commands.indexOf("update_config")).toBeLessThan(
    commands.indexOf("save_theme_copy"),
  );
  expect(harness.onThemes).toHaveBeenCalledWith(FRESH_LIST);
  expect(harness.container.textContent).toContain("Theme file saved:");
});
test("cancelling the native save dialog leaves the library untouched", async () => {
  callMock.mockResolvedValue(null);
  await name("New One");
  await click("Save theme…");
  expect(callMock).not.toHaveBeenCalledWith(
    "save_theme_copy",
    expect.anything(),
  );
  expect(harness.onThemes).not.toHaveBeenCalled();
});
test("existing local names ask before saving, bundled names remain protected", async () => {
  await name("Default");
  expect(harness.button("Save theme…").disabled).toBe(true);
  expect(harness.container.textContent).toContain("built-in");
  await name("Mine");
  await click("Save theme…");
  expect(callMock).not.toHaveBeenCalledWith(
    "choose_settings_file",
    expect.anything(),
  );
  await flush(() => {
    harness.confirmAction().click();
  });
  expect(callMock).toHaveBeenCalledWith(
    "save_theme_copy",
    expect.objectContaining({ name: "mine", overwrite: true }),
  );
});
test("a failed external copy retries the exact captured document", async () => {
  callMock.mockImplementation((command: string) =>
    Promise.resolve(
      command === "choose_settings_file"
        ? "/downloads/retry.json"
        : command === "save_theme_copy"
          ? {
              themes: FRESH_LIST,
              document,
              path: "/readonly/theme.json",
              fileError: "permission denied",
            }
          : null,
    ),
  );
  await name("New One");
  await click("Save theme…");
  expect(harness.container.textContent).toContain("Saved in smabar");
  expect(harness.onThemes).toHaveBeenCalledWith(FRESH_LIST);
  await click("Save file again…");
  expect(callMock).toHaveBeenCalledWith("write_theme_copy", {
    path: "/downloads/retry.json",
    document,
  });
  expect(
    callMock.mock.calls.filter(([command]) => command === "save_theme_copy"),
  ).toHaveLength(1);
});
test("deletion requires confirmation and reports errors without losing the theme", async () => {
  const remove = harness.container.querySelector<HTMLButtonElement>(
    '[aria-label="Delete: My Look"]',
  );
  expect(remove).not.toBeNull();
  await flush(() => remove?.click());
  callMock.mockImplementation((command: string) =>
    command === "delete_theme"
      ? Promise.reject(new Error("read only"))
      : Promise.resolve(null),
  );
  await flush(() => {
    harness.confirmAction().click();
  });
  expect(
    harness.container.querySelector('[role="alert"]')?.textContent,
  ).toContain("read only");
  expect(harness.onThemes).not.toHaveBeenCalled();
});
test("pending save ignores duplicate submission", async () => {
  callMock.mockReturnValue(new Promise(() => undefined));
  await name("New One");
  await click("Save theme…");
  await click("Save theme…");
  expect(
    callMock.mock.calls.filter(
      ([command]) => command === "choose_settings_file",
    ),
  ).toHaveLength(1);
});

test("preview shows the theme's position, rows, width cap and autohide before activation", async () => {
  const theme = {
    ...DROPIN,
    preview: {
      ...DROPIN.preview,
      layout: {
        ...DROPIN.preview.layout,
        position: "top" as const,
        variant: "rows" as const,
        primaryZone: "plugins" as const,
        width: "full" as const,
        maxWidth: 960,
        behavior: "autohide" as const,
      },
    },
  };
  await harness.render(
    <ThemeManager themes={[theme]} onThemes={harness.onThemes} />,
  );
  const card = harness.container.querySelector<HTMLButtonElement>(
    '[aria-label="My Look"]',
  );
  expect(card?.textContent).toContain("Top");
  expect(card?.textContent).toContain("960 px");
  expect(card?.textContent).toContain("Rows");
  expect(card?.textContent).toContain("Auto-hide");
  expect(
    card
      ?.querySelector(".settings-theme-desktop")
      ?.getAttribute("data-position"),
  ).toBe("top");
  const zones = [...(card?.querySelectorAll(".settings-theme-zone") ?? [])];
  expect(zones.map((zone) => zone.getAttribute("data-zone"))).toEqual([
    "plugins",
    "shortcuts",
  ]);
  expect(callMock).not.toHaveBeenCalledWith("update_config", expect.anything());
  await flush(() => card?.click());
  expect(callMock).toHaveBeenCalledWith("update_config", {
    path: "theme",
    value: "mine",
  });
});
