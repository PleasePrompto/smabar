/**
 * Remote player URLs never load in the privileged shell origin. The core
 * supplies a random loopback URL whose static page frames the provider. Native
 * WebView hooks identify the resulting provider document as the desktop app.
 */

let embedRoot = "";

/** Long enough for an accessible description, bounded so it cannot inflate
 * the wrapper URL without limit. */
const MAX_EMBED_TITLE_CHARS = 512;
const MAX_EMBED_SOURCE_CHARS = 8192;

export function setEmbedRoot(root: string): void {
  embedRoot = root;
}

/**
 * Turns a credential-free HTTPS player URL into the isolated wrapper URL.
 * The loopback server validates it again before writing escaped HTML.
 */
export function resolveEmbed(value: string, title: string): string | null {
  if (embedRoot === "") return null;
  const accessibleTitle = title.trim();
  if (
    accessibleTitle === "" ||
    accessibleTitle.length > MAX_EMBED_TITLE_CHARS
  ) {
    return null;
  }
  let source: URL;
  try {
    source = new URL(value);
  } catch {
    return null;
  }
  if (
    source.protocol !== "https:" ||
    source.username !== "" ||
    source.password !== "" ||
    source.href.length > MAX_EMBED_SOURCE_CHARS
  ) {
    return null;
  }
  const payload = new URLSearchParams({
    src: source.href,
    title: accessibleTitle,
  });
  return `${embedRoot}?${payload.toString()}`;
}
