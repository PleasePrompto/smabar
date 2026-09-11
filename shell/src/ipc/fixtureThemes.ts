import defaultTheme from "../../../themes/default.json";
import paperTheme from "../../../themes/paper.json";
import terminalTheme from "../../../themes/terminal.json";
import topbarTheme from "../../../themes/topbar.json";
import { slugifyThemeName } from "../components/settings/model";
import { useSmabar, type ThemeMeta, type ThemeSummary } from "../store/bar";
import { readThemeDocument } from "../theme/document";

/**
 * Browser-dev mirror of the core's theme registry (`themes::BUNDLED`): the
 * compiled-in themes as complete token maps plus the one-shot activation
 * semantics, extended by an in-memory drop-in store for the save/delete/
 * import/export commands. Lives in its own module so the command fixture
 * stays focused.
 */

export interface FixtureTheme {
  tokens: Record<string, string>;
  settings: Record<string, unknown>;
  meta?: ThemeMeta;
}

export const DEFAULT_THEME = readThemeDocument(defaultTheme);

export const FIXTURE_THEMES: Record<string, FixtureTheme> = Object.fromEntries(
  Object.entries({
    default: defaultTheme,
    paper: paperTheme,
    terminal: terminalTheme,
    topbar: topbarTheme,
  }).map(([name, document]) => [name, readThemeDocument(document)]),
);

/** In-memory drop-ins (`~/.smabar/themes/` stand-in), name → document. */
const customThemes = new Map<string, FixtureTheme>();

/** Bundled or custom theme by name (custom patches nothing — full docs). */
export function getFixtureTheme(name: string): FixtureTheme | undefined {
  return FIXTURE_THEMES[name] ?? customThemes.get(name);
}

/** Mirrors `themes::summaries`: every known theme with preview colors. */
export function listThemes(): ThemeSummary[] {
  const active = useSmabar.getState().theme;
  const summarize = (
    name: string,
    theme: FixtureTheme,
    source: ThemeSummary["source"],
  ): ThemeSummary => {
    const resolved: Record<string, string> = {
      ...DEFAULT_THEME.tokens,
      ...theme.tokens,
    };
    return {
      name,
      source,
      active: name === active,
      colors: {
        accent: resolved["--sb-accent"] ?? "",
        accent2: resolved["--sb-accent-2"] ?? "",
        surface: resolved["--sb-bar-bg"] ?? "",
        text: resolved["--sb-text"] ?? "",
      },
      fonts: {
        sans: {
          family: resolved["--sb-font-sans"] ?? "system-ui",
          source: resolved["--sb-font-sans-source"] ?? "system",
        },
        mono: {
          family: resolved["--sb-font-mono"] ?? "monospace",
          source: resolved["--sb-font-mono-source"] ?? "system",
        },
      },
      ...(theme.meta === undefined ? {} : { meta: theme.meta }),
    };
  };
  return [
    ...Object.entries(FIXTURE_THEMES).map(([name, theme]) =>
      summarize(name, theme, "bundled"),
    ),
    ...[...customThemes.entries()].map(([name, theme]) =>
      summarize(name, theme, "dropin"),
    ),
  ];
}

function assertWritable(name: string, overwrite: boolean): void {
  if (name in FIXTURE_THEMES) {
    throw new Error(`"${name}" is a compiled-in theme and read-only`);
  }
  if (!overwrite && customThemes.has(name)) {
    throw new Error(`a theme named "${name}" already exists`);
  }
}

/**
 * Mirrors `save_custom_theme`'s document assembly: active theme resolved,
 * slider overrides baked in, no settings snapshot (browser-dev keeps the
 * behavior side simple). The dispatcher activates the result afterwards.
 */
export function saveCustomTheme(name: string, overwrite: boolean): void {
  assertWritable(name, overwrite);
  const store = useSmabar.getState();
  const base = getFixtureTheme(store.theme);
  customThemes.set(name, {
    tokens: {
      ...DEFAULT_THEME.tokens,
      ...base?.tokens,
      ...store.appearance.tokens,
    },
    settings: {},
  });
}

export function deleteFixtureTheme(name: string): void {
  if (!customThemes.delete(name)) {
    throw new Error(`no drop-in theme "${name}"`);
  }
}

/** Fake filesystem write: returns the path the real command would report. */
export function exportFixtureTheme(name: string, directory = ""): string {
  if (getFixtureTheme(name) === undefined) {
    throw new Error(`no theme "${name}"`);
  }
  const target = directory.trim() || "~/.smabar/themes/export";
  return `${target}/${name}.json`;
}

/**
 * Mirrors `import_theme`: slug from the file stem, bundled names refused,
 * collisions need `overwrite`. Only `.json` paths count as theme documents;
 * the imported look is a fixed sample (there is no filesystem to read).
 */
export function importFixtureTheme(path: string, overwrite: boolean): void {
  if (!path.toLowerCase().endsWith(".json")) {
    throw new Error(
      "invalid theme document: the file contains no theme tokens and no settings block — not a theme document",
    );
  }
  const stem = path.replace(/\\/g, "/").split("/").pop() ?? "";
  const name = slugifyThemeName(stem.replace(/\.json$/i, ""));
  if (name === null) {
    throw new Error(
      `theme name "${stem}" must be non-empty and contain only [a-z0-9-]`,
    );
  }
  assertWritable(name, overwrite);
  customThemes.set(name, {
    tokens: { ...DEFAULT_THEME.tokens, "--sb-accent": "#4db6ac" },
    settings: {},
    meta: { author: "Fixture" },
  });
}

/**
 * Mirrors what `store_install_theme` leaves behind: a drop-in under the
 * listed name, so it shows up under "Saved themes" like the real install.
 */
export function installFixtureTheme(name: string, accent: string): void {
  customThemes.set(name, {
    tokens: { ...DEFAULT_THEME.tokens, "--sb-accent": accent },
    settings: {},
    meta: { author: "Community" },
  });
}

/** Mirrors `get_theme_export_dir` with the default (unconfigured) target. */
export function fixtureExportDir(): { configured: string; effective: string } {
  return { configured: "", effective: "~/.smabar/themes/export" };
}
