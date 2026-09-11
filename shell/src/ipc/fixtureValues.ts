import type { ShortcutConfigEntry, SpecialShortcut } from "../store/bar";
import type { StoreKind } from "./store";

export const SPECIAL_SHORTCUTS = [
  "computer",
  "trash",
] as const satisfies readonly SpecialShortcut[];

export const STORE_KINDS = [
  "plugin",
  "theme",
] as const satisfies readonly StoreKind[];

/**
 * Runtime coercion for the browser-dev fixture. `update_config` takes an
 * untyped value, exactly like the real Tauri command, so the fixture has to
 * check it the same way the core's serde does — a wrong type must be a clear
 * error, not a store silently holding a number where a string belongs.
 */

export function asString(value: unknown, what: string): string {
  if (typeof value !== "string") {
    throw new Error(`fixture: "${what}" must be a string`);
  }
  return value;
}

export function asBoolean(value: unknown, what: string): boolean {
  if (typeof value !== "boolean") {
    throw new Error(`fixture: "${what}" must be a boolean`);
  }
  return value;
}

export function asNumber(value: unknown, what: string): number {
  if (typeof value !== "number") {
    throw new Error(`fixture: "${what}" must be a number`);
  }
  return value;
}

export function asTokenMap(
  value: unknown,
  what: string,
): Record<string, string> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`fixture: "${what}" must be an object of string values`);
  }
  const entries = Object.entries(value as Record<string, unknown>);
  const map: Record<string, string> = {};
  for (const [key, entry] of entries) {
    if (typeof entry !== "string") {
      throw new Error(`fixture: "${what}.${key}" must be a string`);
    }
    map[key] = entry;
  }
  return map;
}

export function asStrings(value: unknown, what: string): string[] {
  if (!Array.isArray(value) || value.some((v) => typeof v !== "string")) {
    throw new Error(`fixture: "${what}" must be an array of strings`);
  }
  // The predicate above proves every element is a string.
  return value.filter((v): v is string => typeof v === "string");
}

export function asChoice<T extends string>(
  value: unknown,
  options: readonly T[],
  what: string,
): T {
  const hit = options.find((option) => option === value);
  if (hit === undefined) {
    throw new Error(`fixture: "${what}" must be one of ${options.join(", ")}`);
  }
  return hit;
}

export function asEntries(value: unknown): ShortcutConfigEntry[] {
  if (!Array.isArray(value)) {
    throw new Error('fixture: "shortcuts.pinned" must be an array');
  }
  return value.map((item: unknown): ShortcutConfigEntry => {
    if (typeof item !== "object" || item === null) {
      throw new Error("fixture: pinned entries must be objects");
    }
    const rec = item as Record<string, unknown>;
    return {
      id: asString(rec.id, "entry.id"),
      desktopId: typeof rec.desktopId === "string" ? rec.desktopId : undefined,
      path: typeof rec.path === "string" ? rec.path : undefined,
      url: typeof rec.url === "string" ? rec.url : undefined,
      special:
        rec.special === undefined
          ? undefined
          : asChoice(rec.special, SPECIAL_SHORTCUTS, "entry.special"),
      label: typeof rec.label === "string" ? rec.label : undefined,
      separator: rec.separator === true ? true : undefined,
    };
  });
}
