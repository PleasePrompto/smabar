// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, test, vi } from "vitest";

import { ShadowHost } from "./PluginContent";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../ipc/call", () => ({ call: callMock }));
vi.mock("../ipc/log", () => ({ reportError: vi.fn() }));

test("native form submits stay inside the shadow root and dispatch the chosen action once", () => {
  (
    globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
  ).IS_REACT_ACT_ENVIRONMENT = true;
  callMock.mockReset().mockResolvedValue(undefined);
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    act(() => {
      root.render(
        <ShadowHost
          pluginId="editor"
          tileId="main"
          popupInstanceId={17}
          html={
            '<form><input data-field="title" value="Draft"><button type="button" data-action="cancel">Cancel</button><button type="submit" data-action="save">Save</button><button type="submit" data-action="publish" data-value="ready">Publish</button></form>'
          }
        />,
      );
    });
    const shadow = host.querySelector("[data-plugin-id]")?.shadowRoot;
    const form = shadow?.querySelector("form");
    const publish = shadow?.querySelector<HTMLButtonElement>(
      '[data-action="publish"]',
    );
    expect(form).toBeInstanceOf(HTMLFormElement);
    const submit = new SubmitEvent("submit", {
      bubbles: true,
      cancelable: true,
      composed: false,
      submitter: publish ?? null,
    });
    act(() => {
      form?.dispatchEvent(submit);
    });
    expect(submit.defaultPrevented).toBe(true);
    expect(callMock).toHaveBeenCalledExactlyOnceWith("plugin_action", {
      pluginId: "editor",
      tileId: "main",
      action: "publish",
      value: "ready",
      popupInstanceId: 17,
    });
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
