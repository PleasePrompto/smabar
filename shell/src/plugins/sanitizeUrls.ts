/** Browsers ignore whitespace/control chars inside URL schemes. */
const stripUrlNoise = (value: string): string =>
  value.replace(/[\u0000-\u0020]/g, "");

const TAURI_INTERNAL_HOSTS = new Set([
  "asset.localhost",
  "ipc.localhost",
  "tauri.localhost",
]);

export const hasJavascriptUrl = (value: string): boolean =>
  stripUrlNoise(value).toLowerCase().includes("javascript:");

export const isHttpUrl = (value: string): boolean => {
  const url = stripUrlNoise(value).toLowerCase();
  return url.startsWith("https://") || url.startsWith("http://");
};

export const isSafeMediaSrc = (
  value: string,
  type: "audio/" | "image/" | "video/" | "text/vtt",
): boolean => {
  const clean = stripUrlNoise(value);
  const lower = clean.toLowerCase();
  if (
    type === "text/vtt"
      ? lower.startsWith("data:text/vtt,") || lower.startsWith("data:text/vtt;")
      : lower.startsWith(`data:${type}`)
  ) {
    return true;
  }
  if (!isHttpUrl(clean)) return false;
  try {
    const host = new URL(clean).hostname.toLowerCase().replace(/\.$/, "");
    return !TAURI_INTERNAL_HOSTS.has(host);
  } catch {
    return false;
  }
};
