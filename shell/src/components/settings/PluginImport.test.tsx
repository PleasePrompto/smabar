// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { PluginImport } from "./PluginImport";
import {
  createThemeManagerTestHarness,
  flush,
  type ThemeManagerTestHarness,
} from "./ThemeManager.testHarness";

const { callMock, stop } = vi.hoisted(() => ({
  callMock: vi.fn(),
  stop: vi.fn(),
}));
vi.mock("../../ipc/call", () => ({ call: callMock }));
vi.mock("../../ipc/store", () => ({
  onStoreProgress: () => Promise.resolve(stop),
}));
const preview = {
  id: "demo",
  name: "Demo",
  version: "1.0.0",
  previousVersion: "1.0.0",
  previousDigest: "old-code",
  archiveSha256: "confirmed-zip",
  community: true,
};
let harness: ThemeManagerTestHarness;
const installed = vi.fn<() => Promise<void>>();
beforeEach(async () => {
  harness = createThemeManagerTestHarness();
  installed.mockReset().mockResolvedValue();
  callMock
    .mockReset()
    .mockImplementation((command: string) =>
      Promise.resolve(
        command === "choose_settings_file"
          ? "/downloads/demo.zip"
          : command === "inspect_plugin_zip"
            ? preview
            : null,
      ),
    );
  await harness.render(<PluginImport onInstalled={installed} />);
});
afterEach(() => {
  harness.dispose();
});

test("same-ID updates require confirmation and explain the change to local ownership", async () => {
  await flush(() => {
    harness.button("Import ZIP…").click();
  });
  expect(harness.container.textContent).toContain(
    "Replace Demo 1.0.0 with 1.0.0?",
  );
  expect(harness.container.textContent).toContain("without Store updates");
  expect(callMock).not.toHaveBeenCalledWith(
    "install_plugin_zip",
    expect.anything(),
  );
  await flush(() => {
    harness.confirmAction().click();
  });
  expect(callMock).toHaveBeenCalledWith("install_plugin_zip", {
    path: "/downloads/demo.zip",
    archiveSha256: "confirmed-zip",
    previousDigest: "old-code",
  });
  expect(installed).toHaveBeenCalledOnce();
  expect(harness.container.querySelector('[role="status"]')).not.toBeNull();
});

test("cancelling file selection performs no inspection or installation", async () => {
  callMock.mockResolvedValue(null);
  await flush(() => {
    harness.button("Import ZIP…").click();
  });
  expect(callMock).toHaveBeenCalledTimes(1);
  expect(installed).not.toHaveBeenCalled();
});

test("a changed ZIP reports the error without claiming installation succeeded", async () => {
  await flush(() => {
    harness.button("Import ZIP…").click();
  });
  callMock.mockImplementation((command: string) =>
    command === "install_plugin_zip"
      ? Promise.reject(new Error("The ZIP changed; select it again"))
      : Promise.resolve(null),
  );
  await flush(() => {
    harness.confirmAction().click();
  });
  expect(
    harness.container.querySelector('[role="alert"]')?.textContent,
  ).toContain("select it again");
  expect(harness.container.querySelector('[role="status"]')).toBeNull();
  expect(installed).not.toHaveBeenCalled();
});
