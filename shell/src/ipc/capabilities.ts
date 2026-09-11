import { uiLog } from "./log";

/**
 * What this webview can actually do, reported once at startup.
 *
 * smabar ships to WebKitGTK, not Chrome, and the gap matters: the plugin
 * kit documents native `<dialog>` + command invokers and the popover API as
 * the way a plugin gets menus and modals without shipping JavaScript. If the
 * runtime lacks one of those, plugin authors would chase a ghost. One info
 * line in the core log answers it — `plugin_logs` without an id shows it.
 */

/** Feature name → probe. Each must be cheap and side-effect free. */
const PROBES: Record<string, () => boolean> = {
  // Native modal: <button commandfor="d" command="show-modal">.
  invokerCommands: () => "commandForElement" in HTMLButtonElement.prototype,
  // Native menus and popups: [popover] + [popovertarget].
  popover: () => HTMLElement.prototype.hasOwnProperty("popover"),
  dialog: () => typeof HTMLDialogElement !== "undefined",
  // Kit styling relies on these.
  lightDark: () => CSS.supports("color", "light-dark(#000, #fff)"),
  oklch: () => CSS.supports("color", "oklch(50% 0.1 200)"),
  colorMix: () =>
    CSS.supports("color", "color-mix(in srgb, red 50%, transparent)"),
  containerQueries: () => CSS.supports("container-type", "inline-size"),
  startingStyle: () => CSS.supports("selector(:popover-open)"),
  has: () => CSS.supports("selector(:has(a))"),
  // Known Chrome-only. See OPTIONAL below: the kit is built without them, so
  // their absence is the expected answer here and not a defect.
  interpolateSize: () => CSS.supports("interpolate-size", "allow-keywords"),
  fieldSizing: () => CSS.supports("field-sizing", "content"),
  // The async Clipboard API needs a secure context; the copy hook falls back
  // to execCommand when it is absent, and this says which path is in use.
  asyncClipboard: () => "clipboard" in navigator,
  // Custom properties registered from an ADOPTED sheet — the one thing that
  // cannot be answered by reading a spec.
  registeredProperty: () => {
    try {
      const sheet = new CSSStyleSheet();
      sheet.replaceSync(
        "@property --sb-probe { syntax: '<number>'; inherits: false; initial-value: 1; }",
      );
      return sheet.cssRules.length > 0;
    } catch {
      return false;
    }
  },
};

/**
 * Behavioural probes: does the runtime ACT on the markup, not just expose the
 * IDL for it. Each builds a throwaway shadow tree, clicks synthetically and
 * reads the resulting state, then throws the tree away.
 *
 * Feature detection alone was not enough — `commandForElement` can exist on
 * the prototype while the activation behaviour does nothing, and that is the
 * difference between a documented plugin pattern working and not.
 */
const BEHAVIOURS: Record<string, () => boolean> = {
  dialogOpensFromInvoker: () =>
    inScratchRoot((root) => {
      root.innerHTML =
        '<button commandfor="d" command="show-modal">o</button>' +
        '<dialog id="d">x</dialog>';
      root.querySelector("button")?.click();
      return root.querySelector("dialog")?.open === true;
    }),
  popoverOpensFromTrigger: () =>
    inScratchRoot((root) => {
      root.innerHTML =
        '<button popovertarget="p">o</button><div id="p" popover>x</div>';
      root.querySelector("button")?.click();
      const popup = root.querySelector<HTMLElement>("#p");
      return popup?.matches(":popover-open") === true;
    }),
  detailsToggles: () =>
    inScratchRoot((root) => {
      root.innerHTML = "<details><summary>s</summary><p>b</p></details>";
      root.querySelector("summary")?.click();
      return root.querySelector("details")?.open === true;
    }),
};

/** Runs `probe` against a detached shadow root and always cleans up. */
function inScratchRoot(probe: (root: ShadowRoot) => boolean): boolean {
  const host = document.createElement("div");
  host.style.display = "none";
  document.body.appendChild(host);
  try {
    return probe(host.attachShadow({ mode: "open" }));
  } finally {
    host.remove();
  }
}

/**
 * Probed for the record, never required.
 *
 * The kit does not use either: `.sb-reveal` animates with the
 * `grid-template-rows: 0fr → 1fr` trick precisely BECAUSE `interpolate-size`
 * is Chromium-only (`ui-kit.css`), and `harvest-f48.py` strips both
 * properties out of every harvested stylesheet (`DEAD_PROPS`). Listing them
 * as MISSING reported a defect that does not exist and named no action —
 * which is the one thing a log line here must always do.
 */
const OPTIONAL = new Set(["interpolateSize", "fieldSizing"]);

/**
 * The capabilities the kit actually needs and this runtime does not have.
 * Pure, so the headline can be checked without a browser to fake.
 */
export function requiredMissing(caps: Record<string, boolean>): string[] {
  return Object.entries(caps)
    .filter(([name, ok]) => !ok && !OPTIONAL.has(name))
    .map(([name]) => name);
}

/** Runs every probe and returns `{name: supported}`. */
export function webviewCapabilities(): Record<string, boolean> {
  const result: Record<string, boolean> = {};
  for (const [name, probe] of Object.entries({ ...PROBES, ...BEHAVIOURS })) {
    try {
      result[name] = probe();
    } catch {
      result[name] = false;
    }
  }
  return result;
}

/** Logs the capability set once, naming what the kit is actually short of. */
export function logWebviewCapabilities(): void {
  const caps = webviewCapabilities();
  const missing = requiredMissing(caps);
  // `fields` still carries every probe, optional ones included — the answer
  // is not lost, it just stops masquerading as a problem in the headline.
  uiLog(
    "info",
    missing.length === 0
      ? "webview supports every feature the kit uses"
      : `webview is MISSING: ${missing.join(", ")}`,
    { fields: caps },
  );
}
