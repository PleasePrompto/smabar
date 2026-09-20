import { call } from "../../ipc/call";
import { reportError } from "../../ipc/log";

/**
 * Persists one settings-panel change through the shared `update_config`
 * command (same dotted paths and validation as the MCP `settings_set` tool).
 * The UI never waits on it: the store follows the resulting config event
 * (in browser dev, the fixture mutates the store directly).
 */
export function setConfig(path: string, value: unknown): void {
  cancelDebouncedConfig(path);
  void persist(path, value).catch(reportError);
}

export interface ConfigWrite {
  path: string;
  value: unknown;
}

/**
 * Applies a reset as ordered writes. Config updates cannot run concurrently:
 * each command must observe the state produced by the preceding command.
 */
export async function setConfigsSequentially(
  writes: readonly ConfigWrite[],
): Promise<void> {
  for (const { path, value } of writes) {
    cancelDebouncedConfig(path);
    await persist(path, value);
  }
}

/** Same cadence as the zone divider's drag persistence. */
const DEBOUNCE_MS = 300;

const timers = new Map<string, { timer: number; value: unknown }>();
const writes = new Set<Promise<void>>();
function persist(path: string, value: unknown): Promise<void> {
  const write = call("update_config", { path, value }).then(() => undefined);
  writes.add(write);
  void write.then(
    () => writes.delete(write),
    () => writes.delete(write),
  );
  return write;
}

/** Save the exact live look, including edits whose debounce has not fired. */
export async function flushConfig(): Promise<void> {
  const pending = [...timers].map(([path, { value }]) => ({ path, value }));
  for (const { path } of pending) cancelDebouncedConfig(path);
  await Promise.all([...writes]);
  await setConfigsSequentially(pending);
}

function cancelDebouncedConfig(path: string): void {
  const pending = timers.get(path);
  if (pending === undefined) return;
  window.clearTimeout(pending.timer);
  timers.delete(path);
}

/**
 * Debounced {@link setConfig} for live controls (sliders): rapid updates to
 * the same path collapse into one write; the last value still lands after
 * the control unmounts (the timeout closure owns it). Distinct paths keep
 * independent timers.
 */
export function setConfigDebounced(path: string, value: unknown): void {
  cancelDebouncedConfig(path);
  timers.set(path, {
    value,
    timer: window.setTimeout(() => {
      timers.delete(path);
      setConfig(path, value);
    }, DEBOUNCE_MS),
  });
}
