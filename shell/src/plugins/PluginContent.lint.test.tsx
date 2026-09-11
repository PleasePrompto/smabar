// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, test, vi } from "vitest";

import { ShadowHost } from "./PluginContent";
import { resetMarkupReports } from "./markupReport";

const logged: { message: string; options?: unknown }[] = [];

vi.mock("../ipc/call", () => ({ call: vi.fn() }));
vi.mock("../ipc/log", () => ({
  reportError: vi.fn(),
  uiLog: (_level: string, message: string, options?: unknown) => {
    logged.push({ message, options });
  },
}));

test("a rendered surface is linted and the report lands in the plugin's log", () => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  resetMarkupReports();
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    act(() => {
      root.render(
        <ShadowHost
          pluginId="demo"
          tileId="main"
          html={
            '<div class="sb-header"><button data-action="refresh"><span data-lucide="refresh-cw"></span></button></div>' +
            '<p style="margin-top: 8px">demo.title</p>'
          }
        />,
      );
    });
    const kinds = logged.map((entry) => entry.message.split(" in ")[0]).sort();
    expect(kinds).toEqual([
      "inline style",
      "raw locale keys rendered as text",
      "unnamed icon-only controls",
    ]);
    expect(
      logged.every(
        (entry) => (entry.options as { pluginId?: string }).pluginId === "demo",
      ),
    ).toBe(true);
  } finally {
    act(() => {
      root.unmount();
    });
    host.remove();
    (
      globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = false;
  }
});
