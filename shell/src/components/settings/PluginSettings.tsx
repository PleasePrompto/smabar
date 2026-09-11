import { t } from "../../i18n/t";
import { useSmabar } from "../../store/bar";
import { Choice, ChoiceGrid, SettingRow, Switch } from "./controls";
import { setConfig, setConfigDebounced } from "./persist";
import { schemaFields, type SchemaField } from "./schemaForm";

/**
 * The fields of one plugin's `settingsSchema`, inside its settings card.
 *
 * Writes go to `plugins.<id>.<key>` through the same update_config path
 * everything else uses; the core persists them and pushes `settings.changed`
 * to the running plugin, so a change applies live.
 */
export function PluginSettings({
  pluginId,
  schema,
}: {
  pluginId: string;
  schema: unknown;
}) {
  const values = useSmabar((s) => s.pluginSettings[pluginId]);
  const fields = schemaFields(schema, values);
  if (fields.length === 0) return null;

  const write = (key: string, value: unknown, debounced = false) => {
    // Update the store first: a debounced write would otherwise let the
    // control snap back to the old value between keystrokes.
    useSmabar.getState().patchPluginSetting(pluginId, key, value);
    const path = `plugins.${pluginId}.${key}`;
    if (debounced) setConfigDebounced(path, value);
    else setConfig(path, value);
  };

  return (
    <>
      {fields.map((field) => (
        <PluginField key={field.key} field={field} onWrite={write} />
      ))}
    </>
  );
}

function PluginField({
  field,
  onWrite,
}: {
  field: SchemaField;
  onWrite: (key: string, value: unknown, debounced?: boolean) => void;
}) {
  // A schema may leave `description` empty. Repeating one generic sentence
  // under every field of the group would be noise, so the row simply gets no
  // help line — its name is all the schema gave us.
  const description = field.description === "" ? undefined : field.description;

  if (field.kind === "boolean") {
    return (
      <SettingRow
        label={field.label}
        description={description}
        control={
          <Switch
            label={field.label}
            checked={field.value}
            onChange={(checked) => {
              onWrite(field.key, checked);
            }}
          />
        }
      />
    );
  }

  if (field.kind === "choice") {
    return (
      <SettingRow label={field.label} description={description}>
        <ChoiceGrid label={field.label}>
          {field.options.map((option) => (
            <Choice
              key={option}
              label={option}
              active={option === field.value}
              onClick={() => {
                onWrite(field.key, option);
              }}
            />
          ))}
        </ChoiceGrid>
      </SettingRow>
    );
  }

  if (field.kind === "unsupported") {
    // Never guess at a shape we cannot present correctly — say where it is
    // editable instead of showing a control that would corrupt the value.
    return (
      <SettingRow
        label={field.label}
        description={description}
        control={
          <span className="sb-faint">{t("settings.plugins.inPlugin")}</span>
        }
      />
    );
  }

  return (
    <SettingRow label={field.label} description={description}>
      <input
        className="sb-input"
        type={field.kind === "number" ? "number" : "text"}
        value={
          field.kind === "list" ? field.value.join(", ") : String(field.value)
        }
        aria-label={field.label}
        placeholder={
          field.kind === "list" ? t("settings.plugins.listHint") : ""
        }
        onChange={(e) => {
          const raw = e.target.value;
          if (field.kind === "number") {
            const parsed = Number(raw);
            if (Number.isFinite(parsed)) onWrite(field.key, parsed, true);
            return;
          }
          if (field.kind === "list") {
            onWrite(
              field.key,
              raw
                .split(",")
                .map((entry) => entry.trim())
                .filter((entry) => entry !== ""),
              true,
            );
            return;
          }
          onWrite(field.key, raw, true);
        }}
      />
    </SettingRow>
  );
}
