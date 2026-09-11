/**
 * Reads a plugin's `settingsSchema` (JSON Schema, straight from its manifest)
 * into the flat list of controls the settings panel can render.
 *
 * Deliberately narrow: only the shapes a generic form can present honestly —
 * text, number, checkbox, a fixed choice, and a list of strings. Anything
 * else (nested objects, lists of objects) is reported as `unsupported` and
 * shown as such, never guessed at and never silently dropped: those belong in
 * the plugin's own flyout, which can offer a real editor (the clock's zone
 * search, crypto's coin list).
 */

export type SchemaField =
  | {
      kind: "text";
      key: string;
      label: string;
      description: string;
      value: string;
    }
  | {
      kind: "number";
      key: string;
      label: string;
      description: string;
      value: number;
    }
  | {
      kind: "boolean";
      key: string;
      label: string;
      description: string;
      value: boolean;
    }
  | {
      kind: "choice";
      key: string;
      label: string;
      description: string;
      value: string;
      options: string[];
    }
  | {
      kind: "list";
      key: string;
      label: string;
      description: string;
      value: string[];
    }
  | { kind: "unsupported"; key: string; label: string; description: string };

type Json = Record<string, unknown>;

const asObject = (value: unknown): Json | undefined =>
  typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Json)
    : undefined;

/** Turns a camelCase / snake_case key into a readable label. */
export function humanize(key: string): string {
  // Sentence case, like every hand-written label in the panel
  // ("Background opacity", not "Background Opacity").
  const spaced = key
    .replace(/[_-]+/g, " ")
    .replace(
      /([a-z0-9])([A-Z])/g,
      (_all, before: string, upper: string) =>
        `${before} ${upper.toLowerCase()}`,
    )
    .trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/**
 * Fields for one plugin. `values` is the plugin's current config object; a
 * missing value falls back to the schema's `default`, so the form shows what
 * the plugin actually uses rather than an empty control.
 */
export function schemaFields(schema: unknown, values: unknown): SchemaField[] {
  const root = asObject(schema);
  const properties = asObject(root?.properties);
  if (properties === undefined) return [];
  const current = asObject(values) ?? {};

  return Object.entries(properties).map(([key, raw]) => {
    const spec = asObject(raw) ?? {};
    const label = typeof spec.title === "string" ? spec.title : humanize(key);
    const description =
      typeof spec.description === "string" ? spec.description : "";
    const value = key in current ? current[key] : spec.default;
    const base = { key, label, description };

    const options = spec.enum;
    if (Array.isArray(options) && options.every((o) => typeof o === "string")) {
      return {
        ...base,
        kind: "choice",
        options,
        value: typeof value === "string" ? value : (options[0] ?? ""),
      };
    }
    switch (spec.type) {
      case "boolean":
        return { ...base, kind: "boolean", value: value === true };
      case "number":
      case "integer":
        return {
          ...base,
          kind: "number",
          value: typeof value === "number" ? value : 0,
        };
      case "string":
        return {
          ...base,
          kind: "text",
          value: typeof value === "string" ? value : "",
        };
      case "array": {
        const items = asObject(spec.items);
        if (items?.type !== "string") {
          return { ...base, kind: "unsupported" };
        }
        const list = Array.isArray(value)
          ? value.filter((entry): entry is string => typeof entry === "string")
          : [];
        return { ...base, kind: "list", value: list };
      }
      default:
        return { ...base, kind: "unsupported" };
    }
  });
}
