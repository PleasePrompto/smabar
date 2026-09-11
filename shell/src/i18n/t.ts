import en from "../../../locales/en.json";

type LocaleMap = Record<string, string>;

const fallback: LocaleMap = en;
let active: LocaleMap = {};

/** Replace the active, core-resolved locale map. */
export function setLocale(map: LocaleMap): void {
  active = map;
}

/** Translate a key; falls back to English, then to the key itself. */
export function t(key: string): string {
  return active[key] ?? fallback[key] ?? key;
}

/** A configured language code that is safe to hand to the browser Intl API. */
export function safeIntlLocale(language: string): string {
  const candidate = language.replaceAll("_", "-");
  try {
    return Intl.DateTimeFormat.supportedLocalesOf(candidate)[0] ?? "en";
  } catch (error: unknown) {
    if (error instanceof RangeError) return "en";
    throw error;
  }
}
