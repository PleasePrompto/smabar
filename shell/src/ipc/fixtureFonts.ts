import type { EnsuredGoogleFont, FontOption, FontSource } from "../theme/fonts";

const system = [
  ["system-ui", "sans-serif", false],
  ["sans-serif", "sans-serif", false],
  ["serif", "serif", false],
  ["ui-monospace", "monospace", true],
  ["monospace", "monospace", true],
  ["Cantarell", "sans-serif", false],
  ["DejaVu Sans", "sans-serif", false],
  ["DejaVu Sans Mono", "monospace", true],
  ["Liberation Sans", "sans-serif", false],
  ["Liberation Mono", "monospace", true],
] as const;

const google = [
  ["noto-sans", "Noto Sans", "sans-serif", false],
  ["roboto-flex", "Roboto Flex", "sans-serif", false],
  ["source-serif-4", "Source Serif 4", "serif", false],
  ["jetbrains-mono", "JetBrains Mono", "monospace", true],
  ["space-mono", "Space Mono", "monospace", true],
  ["ibm-plex-mono", "IBM Plex Mono", "monospace", true],
  ["fraunces", "Fraunces", "display", false],
  ["caveat", "Caveat", "handwriting", false],
] as const;

const cached = new Set<string>();

export function listFixtureFonts(
  query: unknown,
  source: unknown,
  monospaced: unknown,
  limit: unknown,
): FontOption[] {
  if (source !== undefined && source !== "system" && source !== "google") {
    throw new Error('fixture: font source must be "system" or "google"');
  }
  const selected: FontSource = source === "google" ? "google" : "system";
  const needle = typeof query === "string" ? query.trim().toLowerCase() : "";
  const monoOnly = monospaced === true;
  const cap =
    typeof limit === "number" && Number.isFinite(limit)
      ? Math.max(1, Math.min(100, Math.floor(limit)))
      : 50;
  const entries: FontOption[] =
    selected === "system"
      ? system.map(([family, category, mono]) => ({
          id: `system:${family}`,
          family,
          source: "system",
          category,
          monospaced: mono,
          cached: false,
        }))
      : google.map(([id, family, category, mono]) => ({
          id,
          family,
          source: "google",
          category,
          monospaced: mono,
          cached: cached.has(id),
        }));
  return entries
    .filter(
      (font) =>
        (!monoOnly || font.monospaced) &&
        (needle === "" || font.family.toLowerCase().includes(needle)),
    )
    .slice(0, cap);
}

export function ensureFixtureFont(id: unknown): EnsuredGoogleFont {
  if (typeof id !== "string")
    throw new Error("fixture: font id must be a string");
  const match = google.find(([fontId]) => fontId === id);
  if (match === undefined)
    throw new Error(`fixture: unknown Google font "${id}"`);
  cached.add(id);
  return { id, family: match[1], faces: [] };
}
