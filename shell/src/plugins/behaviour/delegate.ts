/**
 * Event delegation for the plugin UI kit.
 *
 * Plugins never ship JavaScript — the shell owns every line of behaviour and
 * plugins reach it through markup alone. That is not a limitation of the kit
 * but of the trust model: plugin HTML is rendered inside the shell's own
 * origin, which holds `window.__TAURI_INTERNALS__` and therefore every
 * command. A plugin runs out-of-process behind JSON-RPC; a script tag in its
 * markup would hand it more power than the process it came from.
 *
 * So the behaviour modules in this folder are the shell's side of the
 * contract: a plugin writes `<button data-sb-copy="#token">` and the shell
 * makes it copy.
 *
 * Two rules make that work across plugin shadow roots:
 *
 * 1. `event.target` is RETARGETED to the shadow host once an event leaves an
 *    open shadow root, so a `document` listener that reads it can never find
 *    the element inside. {@link eventTarget} reads `composedPath()[0]`, which
 *    is the real one. `closest()` from there stays inside the plugin's own
 *    tree — a plugin cannot reach the bar around it.
 * 2. Listeners run in the CAPTURE phase. `PluginContent` stops propagation for
 *    `[data-action]` clicks, and a bubble-phase listener would never see them.
 */

/** What a delegated handler receives: the matched element and the event. */
type Handler<E extends Event = Event> = (
  element: HTMLElement,
  event: E,
) => void;

interface Registration {
  readonly type: string;
  readonly selector: string;
  readonly handler: Handler;
}

const registry: Registration[] = [];

/** A marked ancestor owns the next click and must beat delegated behaviours. */
export const SUPPRESS_DELEGATED_CLICK_ATTR = "data-sb-suppress-click";

/** Callbacks that re-derive DOM state after a plugin re-render. */
const syncs: ((root: ParentNode) => void)[] = [];

/**
 * Registers a delegated listener.
 *
 * The handler receives the closest ancestor of the event's real target that
 * matches `selector`, so it can be authored as if it were bound to that
 * element.
 */
export function behaviour<E extends Event = Event>(
  type: string,
  selector: string,
  handler: Handler<E>,
): void {
  registry.push({ type, selector, handler: handler as Handler });
}

/**
 * Registers a callback that runs on every plugin render.
 *
 * Delegated handlers are stateless — the state lives in the DOM and survives
 * as long as the markup does. A plugin that re-renders replaces its markup,
 * so anything DERIVED from it (a range's fill, a stepper's disabled buttons,
 * an active filter) has to be computed again. That is what this is for.
 */
export function onSync(sync: (root: ParentNode) => void): void {
  syncs.push(sync);
}

/** Runs every {@link onSync} callback over freshly rendered markup. */
export function syncKit(root: ParentNode): void {
  for (const sync of syncs) sync(root);
}

/**
 * The real target of an event, looking through open shadow roots.
 *
 * Returns null for events whose target is not an element (the document
 * itself, a text node in a composed path).
 */
export function eventTarget(event: Event): HTMLElement | null {
  const first = event.composedPath()[0];
  return first instanceof HTMLElement ? first : null;
}

/**
 * The focused element within the same tree as `node`.
 *
 * `document.activeElement` reports the shadow HOST while focus sits inside a
 * plugin's shadow root, which is useless for roving-focus keyboard patterns.
 */
export function activeIn(node: Node): HTMLElement | null {
  const root = node.getRootNode();
  const active =
    root instanceof ShadowRoot || root instanceof Document
      ? root.activeElement
      : null;
  return active instanceof HTMLElement ? active : null;
}

/** Every element matching `selector` inside the same tree as `node`. */
export function queryAll(node: ParentNode, selector: string): HTMLElement[] {
  return Array.from(node.querySelectorAll<HTMLElement>(selector));
}

/**
 * Installs every registered listener. Returns a teardown for tests.
 *
 * One listener per event type carries all selectors for that type, so adding
 * a component costs no extra listener.
 */
export function installKitBehaviour(
  target: EventTarget = document,
): () => void {
  const types = [...new Set(registry.map((entry) => entry.type))];
  const listeners = types.map((type) => {
    const entries = registry.filter((entry) => entry.type === type);
    const listener = (event: Event) => {
      if (
        type === "click" &&
        event
          .composedPath()
          .some(
            (node) =>
              node instanceof HTMLElement &&
              node.hasAttribute(SUPPRESS_DELEGATED_CLICK_ATTR),
          )
      ) {
        return;
      }
      const from = eventTarget(event);
      if (from === null) return;
      for (const entry of entries) {
        const element = from.closest<HTMLElement>(entry.selector);
        if (element !== null) entry.handler(element, event);
      }
    };
    target.addEventListener(type, listener, true);
    return { type, listener };
  });
  return () => {
    for (const { type, listener } of listeners) {
      target.removeEventListener(type, listener, true);
    }
  };
}

/** Reads a number attribute, falling back when it is absent or unparsable. */
export function num(
  value: string | null | undefined,
  fallback: number,
): number {
  if (value === null || value === undefined || value.trim() === "") {
    return fallback;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

/**
 * Moves focus within a list, wrapping at both ends.
 *
 * The shared half of every roving-focus pattern here (menus, comboboxes,
 * option lists). Returns false when there is nothing to move to, so the
 * caller knows whether to consume the key.
 */
export function moveFocus(
  items: HTMLElement[],
  current: HTMLElement | null,
  step: number,
): boolean {
  if (items.length === 0) return false;
  const index = current === null ? -1 : items.indexOf(current);
  const next = index === -1 ? (step > 0 ? 0 : items.length - 1) : index + step;
  const wrapped = ((next % items.length) + items.length) % items.length;
  items[wrapped]?.focus();
  return true;
}
