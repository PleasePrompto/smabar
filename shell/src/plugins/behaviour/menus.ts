/**
 * Dropdown, menubar and context menu — the three menu patterns a plugin
 * cannot build from native markup alone.
 *
 * A plain disclosure is `<details>` and a simple overlay is `[popover]`;
 * both work without any of this. What is here is what those cannot do:
 * keyboard roving between menu items (APG), hover handoff between menubar
 * entries, and opening at cursor coordinates.
 */
import { activeIn, behaviour, moveFocus, queryAll } from "./delegate";

/** Distance a context menu keeps from the window edges. */
const EDGE_MARGIN = 8;

/** Opens or closes one dropdown and mirrors it into its trigger. */
function setDropdown(dropdown: HTMLElement, open: boolean): void {
  dropdown.classList.toggle("is-open", open);
  const toggle = dropdown.querySelector("[data-sb-dropdown-toggle]");
  toggle?.setAttribute("aria-expanded", String(open));
}

/** The tree a plugin's markup lives in — never further out than its root. */
function scopeOf(element: Element): ParentNode | null {
  const root = element.getRootNode();
  return root instanceof ShadowRoot || root instanceof Document ? root : null;
}

/** Light dismiss: closes every open dropdown except one. */
function closeDropdowns(scope: ParentNode, except: HTMLElement | null): void {
  for (const dropdown of queryAll(scope, "[data-sb-dropdown].is-open")) {
    if (dropdown !== except) setDropdown(dropdown, false);
  }
}

behaviour("click", "*", (element) => {
  const scope = scopeOf(element);
  if (scope === null) return;
  const toggle = element.closest("[data-sb-dropdown-toggle]");
  const dropdown = toggle?.closest<HTMLElement>("[data-sb-dropdown]") ?? null;
  closeDropdowns(scope, dropdown);
  if (dropdown !== null) {
    setDropdown(dropdown, !dropdown.classList.contains("is-open"));
  }
});

behaviour("keydown", "*", (element, event) => {
  if (!(event instanceof KeyboardEvent) || event.key !== "Escape") return;
  const scope = scopeOf(element);
  if (scope !== null) closeDropdowns(scope, null);
});

/** The focusable menu items of one dropdown. */
function menuItems(dropdown: ParentNode): HTMLElement[] {
  return queryAll(dropdown, '[role="menuitem"]').filter(
    (item) => !item.hasAttribute("disabled"),
  );
}

/** The dropdown entries of a menubar, in document order. */
function menubarEntries(menubar: HTMLElement): HTMLElement[] {
  return queryAll(menubar, "[data-sb-dropdown]");
}

/** Opens one menubar entry and closes its siblings. */
function switchTo(
  menubar: HTMLElement,
  dropdown: HTMLElement,
  open: boolean,
): void {
  for (const entry of menubarEntries(menubar)) {
    if (entry !== dropdown) setDropdown(entry, false);
  }
  setDropdown(dropdown, open);
}

// Desktop menubar convention: with one menu open, hovering another trigger
// moves the open state there.
behaviour(
  "pointerover",
  "[data-sb-menubar] [data-sb-dropdown-toggle]",
  (toggle) => {
    const menubar = toggle.closest<HTMLElement>("[data-sb-menubar]");
    const dropdown = toggle.closest<HTMLElement>("[data-sb-dropdown]");
    if (menubar === null || dropdown === null) return;
    const open = menubar.querySelector("[data-sb-dropdown].is-open");
    if (open === null || open === dropdown) return;
    switchTo(menubar, dropdown, true);
    toggle.focus();
  },
);

behaviour("keydown", "[data-sb-menubar]", (menubar, event) => {
  if (!(event instanceof KeyboardEvent)) return;
  const from = event.composedPath()[0];
  if (!(from instanceof HTMLElement)) return;
  const dropdown = from.closest<HTMLElement>("[data-sb-dropdown]");
  if (dropdown === null) return;
  const onTrigger = from.hasAttribute("data-sb-dropdown-toggle");

  if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
    const all = menubarEntries(menubar);
    const step = event.key === "ArrowRight" ? 1 : -1;
    const next = all[(all.indexOf(dropdown) + step + all.length) % all.length];
    const nextToggle = next?.querySelector<HTMLElement>(
      "[data-sb-dropdown-toggle]",
    );
    if (next === undefined || nextToggle == null) return;
    event.preventDefault();
    const keepOpen = dropdown.classList.contains("is-open");
    switchTo(menubar, next, keepOpen);
    // Focus came from inside a menu: stay inside the next one.
    const target =
      keepOpen && !onTrigger ? (menuItems(next)[0] ?? nextToggle) : nextToggle;
    target.focus();
    return;
  }

  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    const items = menuItems(dropdown);
    if (items.length === 0) return;
    event.preventDefault();
    if (onTrigger) {
      switchTo(menubar, dropdown, true);
      items[event.key === "ArrowDown" ? 0 : items.length - 1]?.focus();
      return;
    }
    moveFocus(items, from, event.key === "ArrowDown" ? 1 : -1);
    return;
  }

  if (event.key === "Escape" && !onTrigger) {
    setDropdown(dropdown, false);
    dropdown.querySelector<HTMLElement>("[data-sb-dropdown-toggle]")?.focus();
  }
});

/** The context menu currently open, and where focus should return. */
let openMenu: HTMLElement | null = null;
let returnFocus: HTMLElement | null = null;

function closeContextMenu(): void {
  if (openMenu === null) return;
  const menu = openMenu;
  openMenu = null;
  menu.classList.remove("is-open");
  const active = activeIn(menu);
  if (
    returnFocus !== null &&
    returnFocus.isConnected &&
    menu.contains(active)
  ) {
    returnFocus.focus();
  }
  returnFocus = null;
}

/**
 * Opens a context menu at cursor coordinates.
 *
 * The menu is positioned absolutely inside its `[data-sb-context]` wrapper
 * (`position: fixed` would anchor to the nearest transformed surface, not the
 * window), so the coordinates are made relative to that wrapper and clamped
 * to the window so the menu never opens off-screen.
 */
function openContextMenu(
  wrapper: HTMLElement,
  menu: HTMLElement,
  clientX: number,
  clientY: number,
): void {
  closeContextMenu();
  returnFocus = activeIn(menu);
  openMenu = menu;
  menu.classList.add("is-open");
  // Measurable only now that the menu is displayed.
  const anchor = wrapper.getBoundingClientRect();
  const maxX = window.innerWidth - menu.offsetWidth - EDGE_MARGIN;
  const maxY = window.innerHeight - menu.offsetHeight - EDGE_MARGIN;
  const x = Math.max(EDGE_MARGIN, Math.min(clientX, maxX));
  const y = Math.max(EDGE_MARGIN, Math.min(clientY, maxY));
  menu.style.setProperty("--sb-context-x", `${String(x - anchor.left)}px`);
  menu.style.setProperty("--sb-context-y", `${String(y - anchor.top)}px`);
  menuItems(menu)[0]?.focus();
}

behaviour("contextmenu", "*", (element, event) => {
  const wrapper = element.closest<HTMLElement>("[data-sb-context]");
  if (wrapper === null) {
    closeContextMenu();
    return;
  }
  const menu = wrapper.querySelector<HTMLElement>(".sb-context-menu");
  if (menu === null || !(event instanceof MouseEvent)) return;
  event.preventDefault();
  // The bar has its own document-level contextmenu handler in the BUBBLE
  // phase; this one runs during capture. Without stopping here the bar would
  // open its tile menu on top of the plugin's own.
  event.stopPropagation();
  let { clientX, clientY } = event;
  if (clientX === 0 && clientY === 0) {
    // Keyboard invocation (Shift+F10, menu key): anchor to the element.
    const rect = element.getBoundingClientRect();
    clientX = rect.left + EDGE_MARGIN;
    clientY = rect.top + EDGE_MARGIN;
  }
  openContextMenu(wrapper, menu, clientX, clientY);
});

behaviour("click", "*", (element) => {
  if (openMenu === null) return;
  // Outside is a light dismiss; on an item it closes after it activated.
  if (!openMenu.contains(element) || element.closest('[role="menuitem"]')) {
    closeContextMenu();
  }
});

behaviour("keydown", "*", (_element, event) => {
  if (openMenu === null || !(event instanceof KeyboardEvent)) return;
  if (event.key === "Escape") {
    event.preventDefault();
    closeContextMenu();
    return;
  }
  if (event.key === "Tab") {
    closeContextMenu();
    return;
  }
  const items = menuItems(openMenu);
  if (items.length === 0) return;
  const active = activeIn(openMenu);
  const step =
    event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : null;
  if (step !== null) {
    event.preventDefault();
    moveFocus(items, active, step);
  } else if (event.key === "Home") {
    event.preventDefault();
    items[0]?.focus();
  } else if (event.key === "End") {
    event.preventDefault();
    items[items.length - 1]?.focus();
  }
});
