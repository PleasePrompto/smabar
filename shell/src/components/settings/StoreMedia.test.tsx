// @vitest-environment happy-dom
import { act, StrictMode } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { fixtureCall } from "../../ipc/fixture";
import {
  fixtureStoreDetail,
  fixtureStoreOverview,
  resetFixtureStore,
} from "../../ipc/fixtureStore";
import { StorePage } from "./StorePage";
import {
  createThemeManagerTestHarness,
  flush,
  type ThemeManagerTestHarness,
} from "./ThemeManager.testHarness";

const { callMock } = vi.hoisted(() => ({
  callMock:
    vi.fn<
      (command: string, args?: Record<string, unknown>) => Promise<unknown>
    >(),
}));
vi.mock("../../ipc/call", () => ({ call: callMock }));

let harness: ThemeManagerTestHarness;
const imageBase = "https://raw.githubusercontent.com/o/r/commit/";

beforeEach(() => {
  resetFixtureStore();
  callMock.mockImplementation((command, args) =>
    Promise.resolve(fixtureCall(command, args)),
  );
  harness = createThemeManagerTestHarness();
  for (const entry of fixtureStoreOverview().entries) {
    if (entry.id === "docker-status" || entry.kind === "theme") {
      entry.icon = entry.kind === "plugin" ? `${imageBase}icon.png` : null;
      entry.screenshots = Array.from(
        { length: 6 },
        (_, index) => `${imageBase}${String(index)}.png`,
      );
    }
  }
});

afterEach(() => {
  harness.dispose();
});

async function settle() {
  await act(async () => {
    for (let turn = 0; turn < 8; turn += 1) await Promise.resolve();
  });
}

function element<T extends Element>(selector: string, type: new () => T): T {
  const found = harness.container.querySelector(selector);
  if (!(found instanceof type)) throw new Error(`missing ${selector}`);
  return found;
}

async function openDetails(kind: "plugin" | "theme" = "plugin") {
  await harness.render(
    <StrictMode>
      <StorePage kind={kind} />
    </StrictMode>,
  );
  await settle();
  await flush(() => {
    element(
      `[data-store-entry="${kind}:${kind === "plugin" ? "docker-status" : "nord"}"] [data-store-details]`,
      HTMLButtonElement,
    ).click();
  });
  await settle();
}

test("the list loads only icons and avatars; failed images leave stable fallbacks", async () => {
  await harness.render(<StorePage kind="plugin" />);
  await settle();
  const images = [...harness.container.querySelectorAll("img")];
  expect(images.some((image) => image.src === `${imageBase}icon.png`)).toBe(
    true,
  );
  expect(
    images.some(
      (image) => image.src === "https://github.com/containerkat.png?size=64",
    ),
  ).toBe(true);
  expect(images.every((image) => !image.src.endsWith("/0.png"))).toBe(true);
  expect(harness.container.querySelector(".settings-store-gallery")).toBeNull();
  for (const image of images) {
    expect(image.getAttribute("loading")).toBe("lazy");
    expect(image.getAttribute("decoding")).toBe("async");
    expect(image.getAttribute("referrerpolicy")).toBe("no-referrer");
  }
  const icon = element(".settings-store-icon img", HTMLImageElement);
  await flush(() => {
    icon.dispatchEvent(new Event("error"));
  });
  const row = element('[data-store-entry="plugin:docker-status"]', HTMLElement);
  expect(row.querySelector(".settings-store-icon img")).toBeNull();
  expect(row.querySelector(".settings-store-icon svg")).not.toBeNull();
  expect(row.textContent).toContain("Docker status");
});

test("gallery shows ordered screenshots without zoom and aligns plugin section headings", async () => {
  await openDetails();
  const frames = [
    ...harness.container.querySelectorAll(".settings-store-gallery-strip img"),
  ];
  expect(frames.map((frame) => frame.getAttribute("src"))).toEqual(
    Array.from({ length: 6 }, (_, i) => `${imageBase}${String(i)}.png`),
  );
  expect(
    element(".settings-store-keywords", HTMLElement).textContent,
  ).toContain("#docker");
  expect(
    [
      ...harness.container.querySelectorAll(
        ".settings-store-columns > .settings-store-column > .sb-section:first-child",
      ),
    ].map((heading) => heading.textContent),
  ).toEqual(["Readme", "Details"]);
  const gallery = element(".settings-store-gallery-strip", HTMLUListElement);
  expect(gallery.tabIndex).toBe(0);
  expect(gallery.querySelector("button")).toBeNull();
  const image = element(".settings-store-gallery-strip img", HTMLImageElement);
  await flush(() => {
    image.click();
  });
  expect(harness.container.querySelector("dialog")).toBeNull();
  await flush(() => {
    image.dispatchEvent(new Event("error"));
  });
  expect(gallery.firstElementChild?.textContent).toContain("Image unavailable");
});

test("themes show compact details before available readme and releases and release gallery images on exit", async () => {
  await openDetails("theme");
  expect(
    element(".settings-store-icon", HTMLElement).querySelector("img"),
  ).toBeNull();
  expect(
    harness.container.querySelectorAll(".settings-store-gallery-strip > li"),
  ).toHaveLength(6);
  expect(
    harness.container.querySelector(".settings-store-details--compact"),
  ).not.toBeNull();
  expect(
    [
      ...harness.container.querySelectorAll(
        ".settings-store-detail .sb-section",
      ),
    ].map((heading) => heading.textContent),
  ).toEqual(["Screenshots", "Details", "Readme", "Releases"]);
  expect(element(".settings-store-readme", HTMLElement).textContent).toContain(
    "Nord for smabar",
  );
  await flush(() => {
    harness.button("Back to the list").click();
  });
  await settle();
  expect(harness.container.querySelector(".settings-store-gallery")).toBeNull();
  expect(
    [...harness.container.querySelectorAll("img")].some((image) =>
      image.src.startsWith(imageBase),
    ),
  ).toBe(false);
});

test("themes omit empty readme and releases sections", async () => {
  const detail = fixtureStoreDetail("theme", "nord");
  callMock.mockImplementation((command, args) =>
    Promise.resolve(
      command === "store_detail"
        ? { ...detail, readme: null, readmeHtml: null, releases: [] }
        : fixtureCall(command, args),
    ),
  );
  await openDetails("theme");
  expect(
    [
      ...harness.container.querySelectorAll(
        ".settings-store-detail .sb-section",
      ),
    ].map((heading) => heading.textContent),
  ).toEqual(["Screenshots", "Details"]);
  expect(harness.container.querySelector(".settings-store-readme")).toBeNull();
  expect(
    harness.container.querySelector(".settings-store-releases"),
  ).toBeNull();
});
