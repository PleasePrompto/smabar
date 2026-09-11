/**
 * The context menu surface: one panel per level (root + at most one
 * submenu) inside the dedicated overlay window.
 *
 * Keyboard follows the WAI-ARIA menu pattern: roving focus over
 * role="menuitem" entries (disabled entries stay focusable and only refuse
 * activation), Arrow keys wrap and skip separators, Home/End jump,
 * Enter/Space activate, ArrowRight/ArrowLeft enter and leave the submenu,
 * Escape closes one level, Tab dismisses everything. role="menu" is
 * vertical by default, so no aria-orientation.
 */

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { CONTEXT_MENU } from "../../styles/layers";
import { renderIcon } from "../../plugins/icons";
import {
  compactSeparators,
  isEnabled,
  isSeparator,
  isSubmenu,
  nextEnabledIndex,
  type ContextMenuItem,
  type ContextMenuSubmenu,
} from "./model";
import { anchorMenu, anchorSubmenu, type Placement } from "./position";
import { dispatchContextCommand } from "./commands";

/** Hover dwell before a submenu opens (and before a stale one closes). */
const SUBMENU_HOVER_MS = 140;
/** Aligns the submenu's first row with the row that opened it. */
const SUBMENU_ROW_OFFSET_PX = 4;

/** Where a panel opens: at a pointer point, or beside its parent row. */
export type MenuAnchor =
  | { kind: "point"; x: number; y: number }
  | { kind: "submenu"; menuLeft: number; menuRight: number; itemTop: number };

/** Trusted lucide glyph, built by the same serializer plugin markup uses. */
function MenuIcon({ name }: { name: string }) {
  return (
    <span
      className="overlay-menu-icon"
      aria-hidden="true"
      ref={(element) => {
        if (element !== null) {
          element.replaceChildren(renderIcon(name, element.ownerDocument));
        }
      }}
    />
  );
}

interface MenuPanelProps {
  items: readonly ContextMenuItem[];
  /** Accessible name of this panel. */
  label: string;
  anchor: MenuAnchor;
  /** Closes THIS level (Escape, ArrowLeft in a submenu). */
  onClose: () => void;
  /** Dismisses the whole menu (item activated, Tab, outside click). */
  onCloseAll: () => void;
  /** Submenu only: cancels the parent's pending close while hovered. */
  onPointerKeep?: () => void;
  /** Bumped by the parent whenever the KEYBOARD opened this submenu. It is
   *  part of the panel's React key, so every request mounts a fresh panel
   *  that starts with its first entry focused. Zero = opened by hover, which
   *  leaves focus on the parent row (WAI-ARIA menu behavior). */
  focusRequest?: number;
  onMeasure?: (size: { width: number; height: number }) => void;
}

/**
 * One menu level. Renders its own submenu recursively — the model allows
 * exactly one nesting level, so this recursion terminates by construction.
 */
export function MenuPanel({
  items,
  label,
  anchor,
  onClose,
  onCloseAll,
  onPointerKeep,
  focusRequest = 0,
  onMeasure,
}: MenuPanelProps) {
  const list = useMemo(() => compactSeparators(items), [items]);
  const panelRef = useRef<HTMLDivElement>(null);
  const rowRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const timer = useRef(0);
  const [place, setPlace] = useState<Placement | null>(null);
  const [active, setActive] = useState(() =>
    focusRequest > 0 ? nextEnabledIndex(list, -1, 1) : -1,
  );
  const [submenu, setSubmenu] = useState<{
    item: ContextMenuSubmenu;
    anchor: MenuAnchor;
    focusRequest: number;
  } | null>(null);
  const nested = anchor.kind === "submenu";

  // The panel renders at the origin for exactly one layout pass and stays
  // hidden until measured; useLayoutEffect commits before paint, so the
  // placement is never visible as a jump.
  useLayoutEffect(() => {
    const element = panelRef.current;
    if (element === null) return;
    const rect = element.getBoundingClientRect();
    const size = { width: rect.width, height: rect.height };
    onMeasure?.(size);
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    setPlace(
      anchor.kind === "point"
        ? anchorMenu({ x: anchor.x, y: anchor.y }, size, viewport)
        : anchorSubmenu(anchor, size, viewport),
    );
  }, [anchor, onMeasure]);

  const clearTimer = useCallback(() => {
    window.clearTimeout(timer.current);
    timer.current = 0;
  }, []);
  useEffect(() => clearTimer, [clearTimer]);

  // The panel itself takes focus first so keystrokes land here even before
  // an entry is highlighted; afterwards focus roves with `active`. While a
  // submenu is open, IT owns focus. Focus waits for the placement: the panel
  // is visibility:hidden until measured, and hidden elements cannot be
  // focused at all.
  useEffect(() => {
    if (submenu !== null || place === null) return;
    const row = rowRefs.current[active];
    if (row === null || row === undefined) panelRef.current?.focus();
    else row.focus();
  }, [active, submenu, place]);

  const openSubmenu = (
    item: ContextMenuSubmenu,
    index: number,
    key: boolean,
  ) => {
    const panel = panelRef.current;
    const row = rowRefs.current[index];
    if (panel === null || row === null || row === undefined) return;
    const panelRect = panel.getBoundingClientRect();
    const rowRect = row.getBoundingClientRect();
    setSubmenu((current) => ({
      item,
      // Keep the measured anchor while the same submenu stays open, so a
      // keyboard focus request never re-places (and re-animates) it.
      anchor:
        current?.item.id === item.id
          ? current.anchor
          : {
              kind: "submenu",
              menuLeft: panelRect.left,
              menuRight: panelRect.right,
              itemTop: rowRect.top - SUBMENU_ROW_OFFSET_PX,
            },
      focusRequest: key ? (current?.focusRequest ?? 0) + 1 : 0,
    }));
  };

  const activate = (item: ContextMenuItem, index: number) => {
    if (isSeparator(item) || item.disabled === true) return;
    if (isSubmenu(item)) {
      clearTimer();
      openSubmenu(item, index, true);
      return;
    }
    dispatchContextCommand(item.command);
    onCloseAll();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const item = list[active];
    const move = (index: number) => {
      if (index >= 0) setActive(index);
      setSubmenu(null);
    };
    switch (event.key) {
      case "ArrowDown":
        move(nextEnabledIndex(list, active, 1));
        break;
      case "ArrowUp":
        move(nextEnabledIndex(list, active, -1));
        break;
      case "Home":
        move(nextEnabledIndex(list, -1, 1));
        break;
      case "End":
        move(nextEnabledIndex(list, list.length, -1));
        break;
      case "ArrowRight":
        if (item !== undefined && isSubmenu(item) && isEnabled(item)) {
          openSubmenu(item, active, true);
        }
        break;
      case "ArrowLeft":
        if (submenu !== null) setSubmenu(null);
        else if (nested) onClose();
        break;
      case "Enter":
      case " ":
        if (item !== undefined) activate(item, active);
        break;
      case "Escape":
        if (submenu !== null) setSubmenu(null);
        else onClose();
        break;
      case "Tab":
        onCloseAll();
        break;
      default:
        return;
    }
    // Every handled key is ours: the bar must never scroll or move focus on.
    event.preventDefault();
    event.stopPropagation();
  };

  const onRowEnter = (item: ContextMenuItem, index: number) => {
    setActive(isEnabled(item) ? index : -1);
    clearTimer();
    if (isSubmenu(item) && isEnabled(item)) {
      timer.current = window.setTimeout(() => {
        openSubmenu(item, index, false);
      }, SUBMENU_HOVER_MS);
      return;
    }
    // Grace period: the pointer may cross unrelated rows on its way into an
    // open submenu, which sits beside — not below — the panel.
    if (submenu !== null) {
      timer.current = window.setTimeout(() => {
        setSubmenu(null);
      }, SUBMENU_HOVER_MS);
    }
  };

  // One shared leading column as soon as a single entry carries state, so
  // checked and unchecked labels line up.
  const checkable = list.some(
    (item) =>
      !isSeparator(item) && !isSubmenu(item) && item.checked !== undefined,
  );

  const style: CSSProperties = {
    zIndex: CONTEXT_MENU,
    left: place?.left ?? 0,
    top: place?.top ?? 0,
    visibility: place === null ? "hidden" : "visible",
  };

  return (
    <>
      <div
        ref={panelRef}
        className="overlay-menu fixed"
        style={style}
        role="menu"
        aria-label={label}
        tabIndex={-1}
        data-input-region
        onKeyDown={onKeyDown}
        onMouseEnter={onPointerKeep}
        onClick={(event) => {
          event.stopPropagation();
        }}
        onAuxClick={(event) => {
          event.stopPropagation();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
      >
        {list.map((item, index) =>
          isSeparator(item) ? (
            <div key={item.id} className="overlay-menu-separator" role="none" />
          ) : (
            <button
              key={item.id}
              ref={(element) => {
                rowRefs.current[index] = element;
              }}
              type="button"
              role={
                !isSubmenu(item) && item.checked !== undefined
                  ? "menuitemcheckbox"
                  : "menuitem"
              }
              className="overlay-menu-item"
              tabIndex={active === index ? 0 : -1}
              data-danger={
                !isSubmenu(item) && item.danger === true ? "" : undefined
              }
              // Disabled entries stay focusable on purpose (WAI-ARIA menu
              // pattern); aria-disabled communicates the state instead.
              aria-disabled={item.disabled === true ? true : undefined}
              aria-haspopup={isSubmenu(item) ? "menu" : undefined}
              aria-expanded={
                isSubmenu(item) ? submenu?.item.id === item.id : undefined
              }
              aria-checked={
                !isSubmenu(item) && item.checked !== undefined
                  ? item.checked
                  : undefined
              }
              onMouseEnter={() => {
                onRowEnter(item, index);
              }}
              onClick={() => {
                activate(item, index);
              }}
            >
              {checkable && (
                <span className="overlay-menu-check" aria-hidden="true">
                  {!isSubmenu(item) && item.checked === true && (
                    <MenuIcon name="check" />
                  )}
                </span>
              )}
              {item.icon !== undefined && <MenuIcon name={item.icon} />}
              <span className="overlay-menu-label">{item.label}</span>
              {isSubmenu(item) && <MenuIcon name="chevron-right" />}
            </button>
          ),
        )}
      </div>
      {submenu !== null && (
        <MenuPanel
          key={`${submenu.item.id}:${String(submenu.focusRequest)}`}
          items={submenu.item.items}
          label={submenu.item.label}
          anchor={submenu.anchor}
          focusRequest={submenu.focusRequest}
          onPointerKeep={clearTimer}
          onClose={() => {
            setSubmenu(null);
          }}
          onCloseAll={onCloseAll}
        />
      )}
    </>
  );
}
