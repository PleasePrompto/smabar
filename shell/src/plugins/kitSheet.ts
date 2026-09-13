/**
 * Distributes the plugin UI kit stylesheet (ui-kit.css) into plugin shadow
 * roots. One module-global constructable sheet is shared by every root;
 * engines without constructable-stylesheet support get a <style> fallback.
 *
 * Call AFTER ShadowRoot.replaceChildren(): the adopted sheet survives child
 * replacement, the <style> fallback is re-appended per render.
 */

import { uiLog } from "../ipc/log";
import harvestedCss from "../styles/kit-components.css?inline";
import kitCss from "../styles/ui-kit.css?inline";

/**
 * The harvested component CSS FIRST, so its `@layer sb.components` is
 * established before the hand-written kit — which is un-layered and therefore
 * wins every semantic conflict. The generated sheet also orders `sb.reset`
 * below `sb.components`, so shadow-root element normalization cannot flatten
 * chips, menu items or other harvested controls. Existing plugin markup stays
 * untouched, and the harvested rules supply what smabar never had.
 */
const sheetCss = `${harvestedCss}\n${kitCss}`;

/** Classes the stylesheet actually provides. Kept derived from the same CSS
 *  string adopted below so reporting cannot drift from what plugins render. */
const kitClasses = new Set<string>();
const cssWithoutComments = sheetCss.replace(/\/\*[\s\S]*?\*\//g, "");
for (const match of cssWithoutComments.matchAll(/\.(sb-[-_a-zA-Z0-9]+)/g)) {
  const className = match[1];
  if (className !== undefined) kitClasses.add(className);
}

export function isKitClass(className: string): boolean {
  return kitClasses.has(className);
}

const supportsConstructable =
  typeof CSSStyleSheet === "function" &&
  "replaceSync" in CSSStyleSheet.prototype &&
  "adoptedStyleSheets" in ShadowRoot.prototype;

let sharedSheet: CSSStyleSheet | null = null;

function kitSheet(): CSSStyleSheet {
  if (sharedSheet === null) {
    sharedSheet = new CSSStyleSheet();
    sharedSheet.replaceSync(sheetCss);
  }
  return sharedSheet;
}

/** Adopts the UI kit styles into `root` (idempotent). */
export function adoptKit(root: ShadowRoot): void {
  if (supportsConstructable) {
    const sheet = kitSheet();
    if (!root.adoptedStyleSheets.includes(sheet)) {
      root.adoptedStyleSheets = [...root.adoptedStyleSheets, sheet];
    }
    return;
  }
  if (root.querySelector("style[data-sb-kit]") !== null) return;
  // Every shipped engine supports constructable sheets; this branch re-inserts
  // the whole kit per render, so its use must show up in the log.
  uiLog(
    "warn",
    "kit stylesheet: constructable stylesheets unsupported; <style> fallback in use",
  );
  const style = document.createElement("style");
  style.setAttribute("data-sb-kit", "");
  style.textContent = sheetCss;
  root.appendChild(style);
}
