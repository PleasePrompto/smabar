export interface ThemeDocument {
  tokens: Record<string, string>;
  settings: Record<string, unknown>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Splits the checked-in flat theme format into its runtime layers. */
export function readThemeDocument(
  document: Record<string, unknown>,
): ThemeDocument {
  const tokens: Record<string, string> = {};
  for (const [key, value] of Object.entries(document)) {
    if (key.startsWith("--") && typeof value === "string") {
      tokens[key] = value;
    }
  }
  const settings = document.settings;
  return {
    tokens,
    settings: isRecord(settings) ? settings : {},
  };
}
