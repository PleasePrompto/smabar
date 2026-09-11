import { convertFileSrc } from "@tauri-apps/api/core";

import { call } from "../ipc/call";
import { reportError } from "../ipc/log";

export type FontSlot = "sans" | "mono";
export type FontSource = "system" | "google";

export interface FontOption {
  id: string;
  family: string;
  source: FontSource;
  category: "sans-serif" | "serif" | "monospace" | "display" | "handwriting";
  monospaced: boolean;
  cached: boolean;
}

interface LocalFontFace {
  family: string;
  path: string;
  style: "normal" | "italic";
  weight: string;
  unicodeRange?: string;
}

export interface EnsuredGoogleFont {
  id: string;
  family: string;
  faces: LocalFontFace[];
}

export interface FontInstallState {
  status: "idle" | "loading" | "ready" | "error";
  error?: string;
}

export const FONT_TOKENS = [
  "--sb-font-sans",
  "--sb-font-sans-source",
  "--sb-font-mono",
  "--sb-font-mono-source",
] as const;

const IDLE: FontInstallState = { status: "idle" };
const states = new Map<string, FontInstallState>();
const installed = new Map<string, EnsuredGoogleFont>();
const inFlight = new Map<string, Promise<EnsuredGoogleFont>>();
const registeredFaces = new Set<string>();
const listeners = new Set<() => void>();
let activeGoogleSlots = "";
const activeBySlot = new Map<FontSlot, string>();
const GENERIC_FAMILIES = new Set([
  "serif",
  "sans-serif",
  "cursive",
  "fantasy",
  "monospace",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
  "math",
  "fangsong",
]);

export function fontToken(slot: FontSlot): string {
  return `--sb-font-${slot}`;
}

export function fontSourceToken(slot: FontSlot): string {
  return `--sb-font-${slot}-source`;
}

export function isGenericFontFamily(family: string): boolean {
  return GENERIC_FAMILIES.has(family.toLowerCase());
}

/** A family plus a portable generic fallback, safe to persist as CSS. */
export function fontFamilyStack(
  family: string,
  slot: FontSlot,
  category?: FontOption["category"],
): string {
  const generic =
    slot === "mono" || category === "monospace"
      ? "monospace"
      : category === "serif"
        ? "serif"
        : category === "handwriting"
          ? "cursive"
          : "sans-serif";
  if (isGenericFontFamily(family)) {
    return family;
  }
  return `${JSON.stringify(family)}, ${generic}`;
}

export function googleFontId(source: string | undefined): string | null {
  if (!source?.startsWith("google:")) return null;
  const id = source.slice("google:".length).trim();
  return id === "" ? null : id;
}

export function subscribeFontInstall(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getFontInstallState(id: string | null): FontInstallState {
  return id === null ? IDLE : (states.get(id) ?? IDLE);
}

function setState(id: string, state: FontInstallState): void {
  states.set(id, state);
  listeners.forEach((listener) => {
    listener();
  });
}

/** Download (or read from cache) and register one Google family once. */
export function ensureManagedFont(id: string): Promise<EnsuredGoogleFont> {
  const ready = installed.get(id);
  if (ready !== undefined) return Promise.resolve(ready);
  const pending = inFlight.get(id);
  if (pending !== undefined) return pending;

  setState(id, { status: "loading" });
  const request = call<EnsuredGoogleFont>("ensure_google_font", { id })
    .then(async (font) => {
      await registerFaces(font.faces);
      installed.set(id, font);
      setState(id, { status: "ready" });
      for (const slot of ["sans", "mono"] as const) {
        if (activeBySlot.get(slot) !== id) continue;
        const declared = document.documentElement.style.getPropertyValue(
          fontToken(slot),
        );
        document.documentElement.style.setProperty(
          fontToken(slot),
          replaceFontFamily(font.family, declared, slot),
        );
      }
      return font;
    })
    .catch((error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      setState(id, { status: "error", error: message });
      throw error;
    })
    .finally(() => {
      inFlight.delete(id);
    });
  inFlight.set(id, request);
  return request;
}

async function registerFaces(faces: readonly LocalFontFace[]): Promise<void> {
  if (faces.length === 0) return;
  if (typeof FontFace === "undefined") {
    throw new Error(
      "This webview cannot register local fonts; keep the system-font fallback active",
    );
  }
  const loaded: { key: string; face: FontFace }[] = [];
  for (const face of faces) {
    const key = `${face.family}\u0000${face.path}\u0000${face.style}\u0000${face.weight}\u0000${face.unicodeRange ?? ""}`;
    if (registeredFaces.has(key)) continue;
    const url =
      "__TAURI_INTERNALS__" in window ? convertFileSrc(face.path) : face.path;
    const fontFace = await new FontFace(
      face.family,
      `url(${JSON.stringify(url)}) format("woff2")`,
      {
        style: face.style,
        weight: face.weight,
        unicodeRange: face.unicodeRange,
      },
    ).load();
    loaded.push({ key, face: fontFace });
  }
  // Only expose the family after every required subset validated and loaded.
  for (const entry of loaded) {
    document.fonts.add(entry.face);
    registeredFaces.add(entry.key);
  }
}

/** Provision managed fonts declared by the effective theme/token layer. */
export function syncManagedThemeFonts(tokens: Record<string, string>): void {
  const requested = (["sans", "mono"] as const)
    .map((slot) => ({
      slot,
      id: googleFontId(tokens[fontSourceToken(slot)]),
    }))
    .filter(
      (entry): entry is { slot: FontSlot; id: string } => entry.id !== null,
    );
  const signature = requested
    .map(({ slot, id }) => `${slot}:${id}`)
    .sort()
    .join("\u0000");
  if (signature === activeGoogleSlots) return;
  activeGoogleSlots = signature;
  activeBySlot.clear();
  requested.forEach(({ slot, id }) => {
    activeBySlot.set(slot, id);
    // The source id is authoritative for third-party themes. A slower
    // request may finish after another theme/override won the slot; the
    // activeBySlot check in ensureManagedFont keeps it from taking over.
    void ensureManagedFont(id).catch(reportError);
  });
}

/** Replace a third-party theme's declared family with the installed catalog family. */
export function canonicalizeManagedFonts(
  tokens: Record<string, string>,
): Record<string, string> {
  let result = tokens;
  for (const slot of ["sans", "mono"] as const) {
    const id = googleFontId(tokens[fontSourceToken(slot)]);
    const font = id === null ? undefined : installed.get(id);
    if (font === undefined) continue;
    if (result === tokens) result = { ...tokens };
    result[fontToken(slot)] = replaceFontFamily(
      font.family,
      tokens[fontToken(slot)] ?? "",
      slot,
    );
  }
  return result;
}

function replaceFontFamily(
  family: string,
  declared: string,
  slot: FontSlot,
): string {
  const comma = declared.indexOf(",");
  const fallback =
    comma === -1
      ? slot === "mono"
        ? "monospace"
        : "sans-serif"
      : declared.slice(comma + 1).trim();
  return `${JSON.stringify(family)}, ${fallback}`;
}
