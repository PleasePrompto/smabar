/**
 * Small self-contained controls: copy button, number stepper, password
 * reveal, and the range family (single, dual, before/after compare).
 *
 * Every one of them is markup a plugin writes; the behaviour lives here.
 * See {@link file://./delegate.ts} for why plugins never ship the script
 * themselves.
 */
import { t } from "../../i18n/t";
import { uiLog } from "../../ipc/log";

import { behaviour, num, onSync, queryAll } from "./delegate";

/** How long the copy button shows its confirmed state. */
const COPIED_MS = 1500;

const copyTimers = new WeakMap<HTMLElement, number>();
let liveRegion: HTMLElement | null = null;

/**
 * Announces a message to assistive technology.
 *
 * The region lives in the light DOM, outside every plugin shadow root: a
 * plugin re-renders on its own schedule and would otherwise throw the
 * announcement away mid-sentence.
 */
function announce(message: string): void {
  if (liveRegion === null) {
    liveRegion = document.createElement("span");
    liveRegion.className = "sb-sr-only";
    liveRegion.setAttribute("role", "status");
    document.body.append(liveRegion);
  }
  // Cleared first so repeating the same message is announced again.
  liveRegion.textContent = "";
  const region = liveRegion;
  requestAnimationFrame(() => {
    region.textContent = message;
  });
}

/**
 * The text a copy button should put on the clipboard, or null.
 *
 * Every null path is logged: a copy button that does nothing looks identical
 * to one that copied, so without this the author has no way to tell an empty
 * clipboard from a wrong selector.
 */
function copySource(button: HTMLElement): string | null {
  const literal = button.dataset.sbCopyText;
  if (literal !== undefined) return literal;
  const selector = button.dataset.sbCopy;
  if (selector === undefined || selector === "") {
    uiLog("warn", "data-sb-copy has no selector and no data-sb-copy-text");
    return null;
  }
  // Scoped to the plugin's own root — a plugin must not read the bar's DOM.
  const root = button.getRootNode();
  if (!(root instanceof ShadowRoot || root instanceof Document)) return null;
  let source: Element | null = null;
  try {
    source = root.querySelector(selector);
  } catch {
    uiLog("warn", "data-sb-copy selector is not valid CSS", {
      fields: { selector },
    });
    return null;
  }
  if (source === null) {
    uiLog("warn", "data-sb-copy selector matches nothing in this tile", {
      fields: { selector },
    });
    return null;
  }
  if (
    source instanceof HTMLInputElement ||
    source instanceof HTMLTextAreaElement
  ) {
    return source.value;
  }
  return source.textContent.trim();
}

/**
 * The fallback clipboard write, for webviews that refuse the async API.
 *
 * Synchronous, so it still holds the user gesture this click carries — an
 * awaited promise may resolve after the gesture has expired. The textarea
 * has to be focusable: `display: none` or `hidden` leaves the selection
 * empty and the copy a no-op.
 */
function writeClipboardFallback(text: string): boolean {
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute(
    "style",
    "position:fixed;top:0;left:0;width:1px;height:1px;opacity:0;",
  );
  document.body.append(area);
  area.select();
  let copied = false;
  try {
    // eslint-disable-next-line @typescript-eslint/no-deprecated -- the async Clipboard API is refused by WebKitGTK in this window; this is the path that actually copies
    copied = document.execCommand("copy");
  } catch {
    copied = false;
  }
  area.remove();
  return copied;
}

/** Shows the confirmed state on the button for a moment. */
function confirmCopy(button: HTMLElement): void {
  button.classList.add("is-copied");
  clearTimeout(copyTimers.get(button));
  copyTimers.set(
    button,
    window.setTimeout(() => {
      button.classList.remove("is-copied");
    }, COPIED_MS),
  );
  announce(t("kit.copied"));
}

behaviour("click", "[data-sb-copy], [data-sb-copy-text]", (button) => {
  const text = copySource(button);
  if (text === null) return;
  if (!("clipboard" in navigator)) {
    // No async API at all (an insecure context): straight to the fallback.
    if (writeClipboardFallback(text)) confirmCopy(button);
    else {
      uiLog("warn", "clipboard write failed and no Clipboard API is available");
      announce(t("kit.copyFailed"));
    }
    return;
  }
  void navigator.clipboard.writeText(text).then(
    () => {
      confirmCopy(button);
    },
    (error: unknown) => {
      // Refused despite being advertised — the fallback may still work,
      // and the log says which one carried it.
      if (writeClipboardFallback(text)) {
        uiLog(
          "info",
          "clipboard: the async API was refused, execCommand worked",
          {
            fields: { error: String(error) },
          },
        );
        confirmCopy(button);
        return;
      }
      uiLog("warn", "clipboard write was refused by the webview", {
        fields: { error: String(error) },
      });
      announce(t("kit.copyFailed"));
    },
  );
});

/** The number input a stepper wraps. */
function stepperInput(wrapper: HTMLElement): HTMLInputElement | null {
  return wrapper.querySelector<HTMLInputElement>('input[type="number"]');
}

/** Disables the step buttons that would push the value past its bounds. */
function syncStepper(wrapper: HTMLElement): void {
  const input = stepperInput(wrapper);
  if (input === null) return;
  const value = input.valueAsNumber;
  const locked = input.disabled || input.readOnly;
  const down = wrapper.querySelector<HTMLButtonElement>(
    "[data-sb-number-down]",
  );
  const up = wrapper.querySelector<HTMLButtonElement>("[data-sb-number-up]");
  if (down !== null) {
    down.disabled = locked || (input.min !== "" && value <= Number(input.min));
  }
  if (up !== null) {
    up.disabled = locked || (input.max !== "" && value >= Number(input.max));
  }
}

behaviour(
  "click",
  "[data-sb-number-down], [data-sb-number-up]",
  (button, event) => {
    const wrapper = button.closest<HTMLElement>("[data-sb-number]");
    if (wrapper === null) return;
    const input = stepperInput(wrapper);
    if (input === null || input.disabled || input.readOnly) return;
    // A stepper inside a <form> must not submit it.
    event.preventDefault();
    if (button.matches("[data-sb-number-up]")) input.stepUp();
    else input.stepDown();
    // stepUp/stepDown are silent — the sync below listens for these.
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
  },
);

behaviour("input", "[data-sb-number]", syncStepper);

behaviour("click", "[data-sb-password-toggle]", (toggle) => {
  const wrapper = toggle.closest<HTMLElement>(".sb-password");
  const input = wrapper?.querySelector("input") ?? null;
  if (input === null) return;
  const reveal = input.type === "password";
  input.type = reveal ? "text" : "password";
  toggle.setAttribute("aria-pressed", String(reveal));
});

/** Where a range input sits between its bounds, as 0..1. */
function fraction(input: HTMLInputElement): number {
  const min = num(input.min, 0);
  const max = num(input.max, 100);
  return max > min ? (num(input.value, min) - min) / (max - min) : 0;
}

/** Paints a single range's accent fill and its value bubble. */
function syncRange(wrapper: HTMLElement): void {
  const input = wrapper.querySelector<HTMLInputElement>('input[type="range"]');
  if (input === null) return;
  const part = fraction(input);
  wrapper.style.setProperty("--sb-range-fill", `${String(part * 100)}%`);
  wrapper.style.setProperty("--sb-range-pct", String(part));
  const bubble = wrapper.querySelector(".sb-range-wrap__bubble");
  if (bubble !== null) {
    bubble.textContent = input.value + (wrapper.dataset.sbRangeUnit ?? "");
  }
}

behaviour("input", "[data-sb-range]", syncRange);

/**
 * Keeps a dual range consistent: from ≤ to, fill painted, labels current.
 *
 * `changed` is the thumb the user moved — it wins the cross-clamp, so
 * dragging one thumb past the other pushes rather than snaps back.
 */
function syncDualRange(
  wrapper: HTMLElement,
  changed: HTMLElement | null,
): void {
  const inputs = queryAll(wrapper, 'input[type="range"]');
  const from = inputs[0];
  const to = inputs[1];
  if (
    !(from instanceof HTMLInputElement) ||
    !(to instanceof HTMLInputElement)
  ) {
    return;
  }
  if (Number(from.value) > Number(to.value)) {
    if (changed === to) to.value = from.value;
    else from.value = to.value;
  }
  const fromPart = fraction(from);
  const toPart = fraction(to);
  wrapper.style.setProperty("--sb-dualrange-from", String(fromPart));
  wrapper.style.setProperty("--sb-dualrange-to", String(toPart));
  // Both thumbs in the upper half would trap the lower one underneath.
  from.classList.toggle("is-on-top", fromPart > 0.5);
  const unit = wrapper.dataset.sbDualrangeUnit ?? "";
  const outputs = queryAll(wrapper, ".sb-dualrange__value");
  if (outputs[0] !== undefined) outputs[0].textContent = from.value + unit;
  if (outputs[1] !== undefined) outputs[1].textContent = to.value + unit;
}

behaviour("input", "[data-sb-dualrange]", (wrapper, event) => {
  const changed = event.composedPath()[0];
  syncDualRange(wrapper, changed instanceof HTMLElement ? changed : null);
});

/** Positions the before/after divider from its range input. */
function syncCompare(root: HTMLElement): void {
  const input = root.querySelector<HTMLInputElement>('input[type="range"]');
  if (input === null) return;
  root.style.setProperty(
    "--sb-compare-pos",
    `${String(fraction(input) * 100)}%`,
  );
}

behaviour("input", "[data-sb-compare]", syncCompare);

// Everything above derives its state from the markup, so a re-render has to
// recompute it — see onSync in delegate.ts.
onSync((root) => {
  for (const wrapper of queryAll(root, "[data-sb-number]"))
    syncStepper(wrapper);
  for (const wrapper of queryAll(root, "[data-sb-range]")) syncRange(wrapper);
  for (const wrapper of queryAll(root, "[data-sb-dualrange]")) {
    syncDualRange(wrapper, null);
  }
  for (const wrapper of queryAll(root, "[data-sb-compare]")) {
    syncCompare(wrapper);
  }
});
