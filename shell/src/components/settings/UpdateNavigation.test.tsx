// @vitest-environment happy-dom
import { act } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { useSmabar } from "../../store/bar";
import { fixtureCall } from "../../ipc/fixture";
import {
  fixtureStoreOverview,
  resetFixtureStore,
} from "../../ipc/fixtureStore";
import { refreshCommunityBadge } from "../../ipc/store";
import { SettingsPanel } from "./SettingsPanel";
import { StoreUpdateLink } from "./UpdateBadge";
import {
  createThemeManagerTestHarness,
  type ThemeManagerTestHarness,
} from "./ThemeManager.testHarness";

vi.mock("../../ipc/call", () => ({
  call: (command: string, args?: Record<string, unknown>) =>
    Promise.resolve(fixtureCall(command, args)),
}));
vi.mock("../../ipc/surface", () => ({
  closeCurrentSurface: () => Promise.resolve(),
}));

let harness: ThemeManagerTestHarness;
beforeEach(async () => {
  resetFixtureStore();
  useSmabar.setState(useSmabar.getInitialState(), true);
  harness = createThemeManagerTestHarness();
  await refreshCommunityBadge();
});
afterEach(() => {
  harness.dispose();
});

async function settle() {
  await act(async () => {
    for (let i = 0; i < 8; i++) await Promise.resolve();
  });
}

test.each([
  ["plugins/store", "Plugin Store"],
  ["bar/community", "Theme Store"],
])(
  "clicking %s in navigation returns from details to the overview",
  async (group, label) => {
    useSmabar.getState().setSettingsGroup(group);
    await harness.render(<SettingsPanel preview />);
    await settle();
    const details = harness.container.querySelector<HTMLButtonElement>(
      "[data-store-details]",
    );
    expect(details).not.toBeNull();
    await act(async () => {
      details?.click();
      await Promise.resolve();
    });
    await settle();
    expect(
      harness.container.querySelector(".settings-store-detail"),
    ).not.toBeNull();
    const nav = [
      ...harness.container.querySelectorAll<HTMLButtonElement>(
        ".settings-subnav button",
      ),
    ].find((button) => button.textContent === label);
    expect(nav).toBeDefined();
    await act(async () => {
      nav?.click();
      await Promise.resolve();
    });
    await settle();
    expect(
      harness.container.querySelector(".settings-store-detail"),
    ).toBeNull();
    expect(
      harness.container.querySelector(".settings-store-list"),
    ).not.toBeNull();
  },
);

test("the plugin marker continues through the navigation to the installed card and directly to its store detail", async () => {
  useSmabar.getState().setSettingsGroup("plugins");
  await harness.render(<SettingsPanel preview />);
  await settle();
  expect(
    harness.container.querySelector(
      "#settings-tab-plugins .settings-update-dot",
    ),
  ).not.toBeNull();
  expect(
    harness.container.querySelector("#settings-tab-bar .settings-update-dot"),
  ).toBeNull();
  expect(
    [
      ...harness.container.querySelectorAll(
        ".settings-subnav button, .settings-plugin-nav button",
      ),
    ]
      .filter((button) => button.querySelector(".settings-update-dot") !== null)
      .map((button) => button.textContent),
  ).toEqual(
    expect.arrayContaining([
      "Overview",
      "Plugin Store",
      "GitHub notifications",
    ]),
  );
  const pluginButton = [
    ...harness.container.querySelectorAll<HTMLButtonElement>(
      ".settings-plugin-nav button",
    ),
  ].find((button) => button.textContent.includes("GitHub notifications"));
  await act(async () => {
    pluginButton?.click();
    await Promise.resolve();
  });
  await settle();
  const link = harness.container.querySelector<HTMLButtonElement>(
    ".settings-plugin-card .settings-update-link",
  );
  expect(link?.textContent).toContain("1.2.0 → 1.3.0");
  await act(async () => {
    link?.click();
    await Promise.resolve();
  });
  await settle();
  expect(
    harness.container.querySelector(".settings-store-detail h2")?.textContent,
  ).toBe("GitHub notifications");
  expect(
    harness.container.querySelector('[data-store-action="update"]'),
  ).not.toBeNull();
});

test("theme updates use Design navigation and disappear when the overview no longer offers the update", async () => {
  const theme = fixtureStoreOverview().entries.find(
    (entry) => entry.kind === "theme",
  );
  if (theme === undefined) throw new Error("fixture has no theme");
  const updated = {
    ...theme,
    update: { fromVersion: "1.0.0", toVersion: "1.1.0", contentChanged: false },
  };
  useSmabar.setState({ settingsGroup: "design", communityUpdates: [updated] });
  await harness.render(<SettingsPanel preview />);
  await settle();
  expect(
    harness.container.querySelector("#settings-tab-bar .settings-update-dot"),
  ).not.toBeNull();
  expect(
    harness.container.querySelector(
      "#settings-tab-plugins .settings-update-dot",
    ),
  ).toBeNull();
  const marked = [
    ...harness.container.querySelectorAll(
      ".settings-subnav button, .settings-plugin-nav button",
    ),
  ]
    .filter((button) => button.querySelector(".settings-update-dot") !== null)
    .map((button) => button.textContent);
  expect(marked).toEqual(expect.arrayContaining(["Themes", "Theme Store"]));
  act(() => {
    useSmabar.getState().setCommunityUpdates([]);
  });
  expect(harness.container.querySelector(".settings-update-dot")).toBeNull();
});

test("blocked or republished entries keep an actionable update link", async () => {
  const entry = useSmabar.getState().communityUpdates[0];
  if (entry?.update === null || entry === undefined)
    throw new Error("fixture has no update");
  useSmabar.setState({
    communityUpdates: [
      {
        ...entry,
        installable: false,
        update: { ...entry.update, contentChanged: true },
      },
    ],
  });
  await harness.render(<StoreUpdateLink kind={entry.kind} id={entry.id} />);
  expect(harness.container.textContent).toContain("Content changed");
  await act(async () => {
    harness.container.querySelector("button")?.click();
    await Promise.resolve();
  });
  expect(useSmabar.getState().settingsStoreEntry).toEqual({
    kind: entry.kind,
    id: entry.id,
  });
});
