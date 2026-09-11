import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * Resolution of the `sb-asset:` scheme, which lets plugin HTML show files the
 * plugin wrote into its own data directory instead of base64-inlining them
 * into every single render.
 *
 * The scheme is deliberately relative: a plugin names a path inside ITS OWN
 * directory and never learns an absolute one, so `sb-asset:` can never reach
 * another plugin's files or anywhere else on disk. Everything that could
 * escape — `..`, an absolute path, a Windows drive prefix — is rejected here,
 * before the path is ever joined.
 */

/** Set once from `get_ui_state`; empty until then, which disables the scheme. */
let dataRoot = "";

export const ASSET_SCHEME = "sb-asset:";

export function setAssetRoot(root: string): void {
  dataRoot = root;
}

/**
 * The relative path of an `sb-asset:` URL, or null when it is unusable.
 * Segments are validated individually so no separator style slips through.
 */
export function assetRelativePath(value: string): string | null {
  if (!value.startsWith(ASSET_SCHEME)) return null;
  const raw = value.slice(ASSET_SCHEME.length).trim();
  // Query strings and fragments have no meaning on a local file and would
  // only be a second way to smuggle characters past the segment check.
  if (raw === "" || raw.includes("?") || raw.includes("#")) return null;
  // Absolute paths, drive letters and UNC prefixes never address a file
  // inside the plugin's own directory.
  if (raw.startsWith("/") || raw.startsWith("\\") || /^[a-z]:/i.test(raw)) {
    return null;
  }
  const segments = raw.split(/[/\\]/);
  if (segments.some((part) => part === "" || part === "." || part === "..")) {
    return null;
  }
  return segments.join("/");
}

/**
 * `sb-asset:logo.png` → a URL the webview can load, or null when the path is
 * unsafe, the plugin is unknown, or the data root has not arrived yet.
 */
export function resolveAsset(
  pluginId: string | undefined,
  value: string,
): string | null {
  if (dataRoot === "" || pluginId === undefined || pluginId === "") return null;
  const relative = assetRelativePath(value);
  if (relative === null) return null;
  // convertFileSrc reads window.__TAURI_INTERNALS__ and throws without it —
  // in plain-browser dev there is no asset protocol to address anyway.
  if (!("__TAURI_INTERNALS__" in window)) return null;
  return convertFileSrc(`${dataRoot}/${pluginId}/${relative}`);
}
