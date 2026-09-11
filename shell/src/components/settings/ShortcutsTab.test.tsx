// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useSmabar } from "../../store/bar";
import { ShortcutsTab } from "./ShortcutsTab";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../../ipc/call", () => ({ call: callMock }));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("reportError", vi.fn());
  useSmabar.setState(useSmabar.getInitialState(), true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  callMock.mockReset();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function type(field: HTMLInputElement, value: string) {
  Reflect.set(HTMLInputElement.prototype, "value", value, field);
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

test("a slower old application search cannot replace newer results", async () => {
  callMock.mockImplementation(
    (command: string, args?: Record<string, unknown>) => {
      if (command === "get_app_icon") return Promise.resolve(null);
      const query = args?.query;
      const app =
        query === "new"
          ? { desktopId: "new.desktop", name: "New result" }
          : { desktopId: "old.desktop", name: "Old result" };
      const delay = query === "new" ? 10 : 1_000;
      return new Promise((resolve) => {
        window.setTimeout(() => {
          resolve([app]);
        }, delay);
      });
    },
  );

  act(() => {
    root.render(<ShortcutsTab />);
  });
  expect(container.textContent).toContain("Searching applications…");
  await act(async () => {
    await vi.advanceTimersByTimeAsync(200);
  });

  const search = container.querySelector<HTMLInputElement>(
    'input[type="search"]',
  );
  if (search === null) throw new Error("application search is missing");
  act(() => {
    type(search, "new");
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(210);
  });
  expect(container.textContent).toContain("New result");
  expect(container.textContent).not.toContain("Old result");

  await act(async () => {
    await vi.advanceTimersByTimeAsync(790);
  });
  expect(container.textContent).toContain("New result");
  expect(container.textContent).not.toContain("Old result");
});

test("application results preserve their platform source for icons and pinning", async () => {
  const windowsPath = String.raw`C:\ProgramData\Microsoft\Windows\Start Menu\Blender.lnk`;
  const macosPath = "/Applications/Names + spaces.app";
  callMock.mockImplementation((command: string) => {
    if (command === "list_apps") {
      return Promise.resolve([
        { desktopId: "org.gnome.Calculator.desktop", name: "Calculator" },
        { path: windowsPath, name: "Blender" },
        { path: macosPath, name: "macOS app" },
      ]);
    }
    return Promise.resolve(null);
  });

  act(() => {
    root.render(<ShortcutsTab />);
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(200);
  });

  const pins = container.querySelectorAll<HTMLButtonElement>(
    'button[aria-label="Pin"]',
  );
  expect(pins).toHaveLength(3);
  act(() => {
    pins[0]?.click();
    pins[1]?.click();
    pins[2]?.click();
  });

  expect(callMock).toHaveBeenCalledWith("get_app_icon", {
    desktopId: "org.gnome.Calculator.desktop",
  });
  expect(callMock).toHaveBeenCalledWith("get_app_icon", {
    path: windowsPath,
  });
  expect(callMock).toHaveBeenCalledWith("pin_shortcut", {
    desktopId: "org.gnome.Calculator.desktop",
  });
  expect(callMock).toHaveBeenCalledWith("pin_shortcut", {
    path: windowsPath,
  });
  expect(callMock).toHaveBeenCalledWith("get_app_icon", { path: macosPath });
  expect(callMock).toHaveBeenCalledWith("pin_shortcut", { path: macosPath });
});

test("special item buttons pin the exact persisted source", () => {
  callMock.mockImplementation((command: string) =>
    command === "list_apps" ? Promise.resolve([]) : Promise.resolve(null),
  );
  act(() => {
    root.render(<ShortcutsTab />);
  });

  const computer = container.querySelector<HTMLButtonElement>(
    'button[aria-label="Computer"]',
  );
  const trash = container.querySelector<HTMLButtonElement>(
    'button[aria-label="Trash"]',
  );
  if (computer === null || trash === null) {
    throw new Error("special shortcut buttons are missing");
  }
  act(() => {
    computer.click();
    trash.click();
  });

  expect(callMock).toHaveBeenCalledWith("pin_special_shortcut", {
    special: "computer",
  });
  expect(callMock).toHaveBeenCalledWith("pin_special_shortcut", {
    special: "trash",
  });
});
