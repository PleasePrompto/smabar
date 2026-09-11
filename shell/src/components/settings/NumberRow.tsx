/** A bounded integer input for values a slider would misrepresent (a port). */
export function NumberRow({
  id,
  label,
  min,
  max,
  value,
  disabled = false,
  onChange,
}: {
  id?: string;
  label: string;
  min: number;
  max: number;
  value: number;
  disabled?: boolean;
  onChange: (value: number) => void;
}) {
  return (
    <input
      id={id}
      className="sb-input"
      type="number"
      min={min}
      max={max}
      value={value}
      disabled={disabled}
      aria-label={label}
      onChange={(e) => {
        const parsed = Number(e.target.value);
        // Out-of-range keystrokes are unavoidable while typing ("7" on the way
        // to "7627"); only a value the config would accept is written.
        if (Number.isInteger(parsed) && parsed >= min && parsed <= max) {
          onChange(parsed);
        }
      }}
    />
  );
}
