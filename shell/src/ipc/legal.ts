import { call } from "./call";

/**
 * The terms-of-use gate's contract with the core (`commands/legal.rs`): the
 * bundled documents rendered for the configured language, whether the
 * current terms still need accepting, and the two ways out of the gate.
 */

export interface LegalDocument {
  title: string;
  /** The document's `updated` front matter (`YYYY-MM-DD`); null for the license. */
  updated: string | null;
  /** Rendered by the core's own converter: raw HTML dropped, links https only. */
  html: string;
}

export interface LegalStatus {
  /** The bundled terms are not accepted; the bar shows only the legal tile. */
  required: boolean;
  /** The bundled terms' `updated` date — the version an acceptance names. */
  termsVersion: string;
  privacyVersion: string;
  /** Unix milliseconds of the last acceptance; null on a fresh profile. */
  acceptedAt: number | null;
  terms: LegalDocument;
  privacy: LegalDocument;
  license: LegalDocument;
}

/** Payload of `legal-changed`, sent to every window after an acceptance. */
export interface LegalChanged {
  required: boolean;
}

export function legalStatus(): Promise<LegalStatus> {
  return call<LegalStatus>("legal_status");
}

/** Records the acceptance of the bundled terms; rejects with the reason. */
export function acceptLegal(): Promise<LegalStatus> {
  return call<LegalStatus>("legal_accept");
}

/** Quits smabar; the promise only settles if the core did not exit first. */
export async function declineLegal(): Promise<void> {
  await call("legal_decline");
}
