// Shared harness for the OverlaySurface test files: mounts the surface with
// two registered tiles, resets store and mocks before every test, and binds
// the event and pull helpers to the file's hoisted mocks. The vi.mock calls
// stay in each test file because vitest hoists them per file.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, vi, type Mock } from "vitest";

import { registerTile, unregisterPluginTiles } from "./registry";
import { useSmabar } from "../store/bar";
import { recordMemoryUi } from "../ipc/memoryProbe";
import { setEmbedRoot } from "../plugins/embeds";
import { OverlaySurface } from "./OverlaySurface";

export type Listener = (event: { payload: unknown }) => void;

export interface OverlayMocks {
  listeners: Map<string, Listener>;
  reportMeasureMock: Mock;
  pinMock: Mock;
  invokeMock: Mock;
  takeQueue: unknown[][];
}

export const systemContent = { hover: null, flyout: "SystemInfo content" };
export const kitContent = { hover: null, flyout: "UI Kit content" };

/** Signals `generation`; the pull for it answers with `rendered`. */
export type Pushed = { generation: number } & Record<string, unknown>;

type ActEnvironment = typeof globalThis & {
  IS_REACT_ACT_ENVIRONMENT?: boolean;
};

export function installOverlayHarness(
  mocks: OverlayMocks,
  onMount: (host: HTMLDivElement, root: Root) => void,
): {
  emit: (name: string, payload: unknown) => void;
  deliver: (rendered: Pushed[], generation?: number) => Promise<void>;
} {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(async () => {
    (globalThis as ActEnvironment).IS_REACT_ACT_ENVIRONMENT = true;
    mocks.listeners.clear();
    mocks.reportMeasureMock.mockClear();
    mocks.pinMock.mockClear();
    mocks.takeQueue.length = 0;
    mocks.invokeMock.mockReset();
    mocks.invokeMock.mockImplementation((command: string) =>
      command === "take_plugin_ui"
        ? Promise.resolve(mocks.takeQueue.shift() ?? [])
        : Promise.reject(new Error(`unexpected command: ${command}`)),
    );
    useSmabar.setState(useSmabar.getInitialState(), true);
    vi.mocked(recordMemoryUi).mockClear();
    registerTile({
      id: "plugin:systeminfo:system",
      pluginId: "systeminfo",
      tile: { id: "system", name: "System", hasFlyout: true },
      meta: { name: "System" },
    });
    registerTile({
      id: "plugin:kitshow:kit",
      pluginId: "kitshow",
      tile: { id: "kit", name: "UI Kit", hasFlyout: true },
      meta: { name: "UI Kit" },
    });
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    onMount(host, root);
    await act(async () => {
      root.render(<OverlaySurface />);
      await Promise.resolve();
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    host.remove();
    setEmbedRoot("");
    unregisterPluginTiles("systeminfo");
    unregisterPluginTiles("kitshow");
    (globalThis as ActEnvironment).IS_REACT_ACT_ENVIRONMENT = false;
  });

  const emit = (name: string, payload: unknown): void => {
    const listener = mocks.listeners.get(name);
    if (listener === undefined) throw new Error(`missing ${name} listener`);
    act(() => {
      listener({ payload });
    });
  };

  const deliver = async (
    rendered: Pushed[],
    generation = rendered[0]?.generation,
  ): Promise<void> => {
    mocks.takeQueue.length = 0;
    mocks.takeQueue.push(rendered);
    const listener = mocks.listeners.get("plugin-ui-overlay");
    if (listener === undefined) throw new Error("missing overlay listener");
    await act(async () => {
      listener({ payload: { generation } });
      for (let turn = 0; turn < 8; turn += 1) await Promise.resolve();
    });
  };

  return { emit, deliver };
}
