import { invoke } from "@tauri-apps/api/core";

/**
 * Runs a Tauri command. Outside the Tauri window (plain-browser dev) the
 * command is served by the fixture handlers instead, so the settings panel
 * stays fully usable there. The fixture module is loaded lazily — prod
 * bundles never pull it in on the invoke path.
 */
export async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if ("__TAURI_INTERNALS__" in window) {
    return invoke<T>(command, args);
  }
  const { fixtureCall } = await import("./fixture");
  // Same trust boundary as invoke<T>: the caller declares the result shape.
  return fixtureCall(command, args) as T;
}
