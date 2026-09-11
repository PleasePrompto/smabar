import { useSmabar } from "../../store/bar";
import type { ConfigWrite } from "./persist";
import { setConfigDebounced } from "./persist";

/**
 * Writes `appearance.tokens` overrides (--sb-* custom properties layered over
 * the active theme). The store is updated immediately for live feedback;
 * persistence writes the WHOLE map in one debounced call — parallel per-token
 * writes would race in the core (each one recomputes from the then-current
 * config).
 */
export function writeTokens(patch: Record<string, string>): void {
  const store = useSmabar.getState();
  const tokens = { ...store.appearance.tokens, ...patch };
  store.setAppearance({ ...store.appearance, tokens });
  setConfigDebounced("appearance.tokens", tokens);
}

/**
 * Hands `keys` back to the theme: drops them from the override map, live and
 * then persisted. This is the only way to undo a single override — before it
 * existed the whole section had to be reset.
 */
export function dropTokens(keys: readonly string[]): void {
  const store = useSmabar.getState();
  const tokens = without(store.appearance.tokens, keys);
  store.setAppearance({ ...store.appearance, tokens });
  setConfigDebounced("appearance.tokens", tokens);
}

/**
 * The same reduction as a write, for a section reset's write list.
 *
 * Every section resets only the tokens it actually shows, so resetting the
 * bar cannot silently discard the colours picked under Design.
 */
export function clearTokens(keys: readonly string[]): ConfigWrite {
  const tokens = useSmabar.getState().appearance.tokens;
  return { path: "appearance.tokens", value: without(tokens, keys) };
}

function without(
  tokens: Record<string, string>,
  keys: readonly string[],
): Record<string, string> {
  const dropped = new Set(keys);
  return Object.fromEntries(
    Object.entries(tokens).filter(([key]) => !dropped.has(key)),
  );
}
