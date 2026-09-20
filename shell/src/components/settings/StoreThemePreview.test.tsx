// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { fixtureCall } from "../../ipc/fixture";
import {
  fixtureStoreOverview,
  resetFixtureStore,
} from "../../ipc/fixtureStore";
import { StoreThemePreview } from "./StoreThemePreview";
import {
  createThemeManagerTestHarness,
  flush,
  type ThemeManagerTestHarness,
} from "./ThemeManager.testHarness";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../../ipc/call", () => ({ call: callMock }));
let harness: ThemeManagerTestHarness;
let reveal: () => void;
const details = vi.fn();
function entry() {
  const theme = fixtureStoreOverview().entries.find(
    (item) => item.kind === "theme",
  );
  if (!theme) throw new Error("missing theme fixture");
  return theme;
}
beforeEach(() => {
  harness = createThemeManagerTestHarness();
  resetFixtureStore();
  details.mockClear();
  callMock.mockReset();
  callMock.mockImplementation(
    (command: string, args?: Record<string, unknown>) =>
      Promise.resolve(fixtureCall(command, args)),
  );
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(callback: (items: { isIntersecting: boolean }[]) => void) {
        reveal = () => {
          callback([{ isIntersecting: true }]);
        };
      }
      observe = vi.fn();
      disconnect = vi.fn();
    },
  );
});
afterEach(() => {
  harness.dispose();
  vi.unstubAllGlobals();
});
test("loads only visible themes, shows four colors and opens details without installing", async () => {
  const theme = entry();
  await harness.render(<StoreThemePreview entry={theme} onDetails={details} />);
  expect(callMock).not.toHaveBeenCalled();
  await flush(() => {
    reveal();
  });
  expect(callMock).toHaveBeenCalledWith("store_theme_preview", {
    name: theme.id,
    expectedCommit: theme.commit,
  });
  expect(
    harness.container.querySelector(".settings-theme-desktop"),
  ).not.toBeNull();
  expect(
    harness.container.querySelectorAll('.settings-theme-swatches [role="img"]'),
  ).toHaveLength(4);
  await flush(() => {
    harness.container
      .querySelector<HTMLButtonElement>(".settings-theme-preview-trigger")
      ?.click();
  });
  expect(details).toHaveBeenCalledOnce();
  expect(callMock).toHaveBeenCalledTimes(1);
});
test("a failed preview has an explicit retry and preserves the install action", async () => {
  callMock.mockRejectedValueOnce(new Error("offline"));
  await harness.render(
    <StoreThemePreview entry={entry()}>
      <button type="button">Install</button>
    </StoreThemePreview>,
  );
  await flush(() => {
    reveal();
  });
  expect(harness.container.textContent).toContain("Preview unavailable");
  expect(harness.button("Install").disabled).toBe(false);
  await flush(() => {
    harness.button("Retry").click();
  });
  await flush(() => {
    reveal();
  });
  expect(
    harness.container.querySelector(".settings-theme-desktop"),
  ).not.toBeNull();
  expect(harness.container.textContent).not.toContain("offline");
});
