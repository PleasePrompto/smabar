/**
 * Types for the per-icon ESM modules of lucide-react. The package ships them
 * without declaration files; importing `__iconNode` directly keeps the icon
 * registry tree-shakeable (only imported icons reach the bundle).
 */
declare module "lucide-react/dist/esm/icons/*.mjs" {
  import type { IconNode } from "lucide-react";

  export const __iconNode: IconNode;
}
