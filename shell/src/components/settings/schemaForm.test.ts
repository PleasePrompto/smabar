import { expect, test } from "vitest";

import { humanize, schemaFields } from "./schemaForm";

const SCHEMA = {
  type: "object",
  properties: {
    label: {
      type: "string",
      description: "Shown on the tile",
      default: "Berlin",
    },
    refreshMinutes: { type: "number", default: 10 },
    showSeconds: { type: "boolean", default: true },
    units: { type: "string", enum: ["metric", "imperial"], default: "metric" },
    coins: { type: "array", items: { type: "string" }, default: ["bitcoin"] },
    zones: { type: "array", items: { type: "object" } },
    nested: { type: "object", properties: {} },
  },
};

test("humanize turns manifest keys into readable labels", () => {
  expect(humanize("refreshMinutes")).toBe("Refresh minutes");
  expect(humanize("alert_percent")).toBe("Alert percent");
  expect(humanize("label")).toBe("Label");
});

test("each supported schema type maps to one control", () => {
  const kinds = Object.fromEntries(
    schemaFields(SCHEMA, {}).map((field) => [field.key, field.kind]),
  );
  expect(kinds).toEqual({
    label: "text",
    refreshMinutes: "number",
    showSeconds: "boolean",
    units: "choice",
    coins: "list",
    // Shapes a generic form cannot present honestly are reported, never
    // guessed at — they stay editable in the plugin's own flyout.
    zones: "unsupported",
    nested: "unsupported",
  });
});

test("stored values win over schema defaults", () => {
  const fields = schemaFields(SCHEMA, {
    label: "Tokyo",
    showSeconds: false,
    coins: ["solana", "ethereum"],
  });
  const byKey = Object.fromEntries(fields.map((f) => [f.key, f]));
  expect(byKey.label).toMatchObject({ value: "Tokyo" });
  expect(byKey.showSeconds).toMatchObject({ value: false });
  expect(byKey.coins).toMatchObject({ value: ["solana", "ethereum"] });
  // Untouched keys fall back to the manifest default, so the form shows what
  // the plugin actually uses rather than an empty control.
  expect(byKey.refreshMinutes).toMatchObject({ value: 10 });
  expect(byKey.units).toMatchObject({ value: "metric" });
});

test("descriptions become the help line, titles the label", () => {
  const [label] = schemaFields(
    {
      properties: {
        label: { type: "string", title: "City", description: "Help" },
      },
    },
    {},
  );
  expect(label).toMatchObject({ label: "City", description: "Help" });
});

test("a plugin without a usable schema yields no form", () => {
  expect(schemaFields(undefined, {})).toEqual([]);
  expect(schemaFields({}, {})).toEqual([]);
  expect(schemaFields({ type: "object" }, {})).toEqual([]);
  expect(schemaFields("not a schema", {})).toEqual([]);
});

test("wrongly typed stored values fall back instead of crashing", () => {
  const fields = schemaFields(SCHEMA, {
    label: 42,
    refreshMinutes: "soon",
    coins: ["ok", 7, null],
  });
  const byKey = Object.fromEntries(fields.map((f) => [f.key, f]));
  expect(byKey.label).toMatchObject({ value: "" });
  expect(byKey.refreshMinutes).toMatchObject({ value: 0 });
  expect(byKey.coins).toMatchObject({ value: ["ok"] });
});
