/**
 * The ONE context-menu entry point of the shell.
 *
 * A single document-level `contextmenu` listener suppresses the webview's
 * native menu (a taskbar must never offer "Reload"/"Inspect"), resolves what
 * was hit through the composed event path, and hands the resulting items to
 * the store. Every area — shortcut tiles, tile tiles, the bar surface and
 * plugin markup — goes through this path; there is no second implementation.
 *
 * Suppressing the native menu is OUR job: Tauri v2 does not disable the
 * webview context menu and offers no setting for it — the documented fix is
 * exactly this preventDefault.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { t } from "../../i18n/t";
import { useSmabar } from "../../store/bar";
import { MenuPanel, type MenuAnchor } from "./ContextMenu";
import {
  isEditableTarget,
  resolveContextTargets,
  type ContextTarget,
} from "./contextTarget";
import { barMenu, shortcutMenu, tileMenu } from "./menus";
import type { ContextMenuItem } from "./model";
import { CONTEXT_ITEMS_ATTR, parseContextItems } from "./pluginMenu";
import { reportError } from "../../ipc/log";
import { cleanupListeners } from "../../ipc/listeners";
import {
  closeContextMenuSurface,
  openContextMenuSurface,
  reportContextMenuMeasure,
  type MenuPlacement,
  type MenuRequest,
} from "../../ipc/overlay";

/** The menu one target offers, or null when it has none. */
function itemsFor(target: ContextTarget): ContextMenuItem[] | null {
  switch (target.kind) {
    case "plugin": {
      const items = parseContextItems(
        target.spec,
        target.pluginId,
        target.tileId,
      );
      if (items === null) {
        // Actionable for the plugin author; the caller falls back to the
        // tile menu so the right-click still does something sensible.
        reportError(
          new Error(
            `plugin "${target.pluginId}" tile "${target.tileId}": ` +
              `${CONTEXT_ITEMS_ATTR} is not a usable JSON array of menu items`,
          ),
        );
      }
      return items;
    }
    case "shortcut": {
      const pin = useSmabar
        .getState()
        .shortcuts.pinned.find((entry) => entry.id === target.id);
      return pin === undefined ? null : shortcutMenu(pin);
    }
    case "tile":
      return tileMenu(target.id);
    case "bar":
      return barMenu();
  }
}

/**
 * Where the menu opens. A keyboard-triggered context menu (Menu key,
 * Shift+F10) carries no pointer position — browsers report detail 0 with
 * 0/0 coordinates — so it anchors under the focused element instead.
 */
function anchorPoint(event: MouseEvent): { x: number; y: number } {
  if (event.detail === 0 && event.clientX === 0 && event.clientY === 0) {
    const focused = document.activeElement;
    if (focused instanceof HTMLElement) {
      const rect = focused.getBoundingClientRect();
      return { x: rect.left, y: rect.bottom };
    }
  }
  return { x: event.clientX, y: event.clientY };
}

export function ContextMenuLayer({ surface }: { surface: "bar" | "overlay" }) {
  const [menu, setMenu] = useState<MenuRequest | null>(null);
  const [placement, setPlacement] = useState<MenuPlacement | null>(null);

  useEffect(() => {
    if (surface !== "overlay") return;
    return cleanupListeners([
      listen<MenuRequest>("surface-menu", (event) => {
        setMenu(event.payload);
        setPlacement(null);
      }),
      listen<MenuPlacement>("menu-placement", (event) => {
        setPlacement(event.payload);
      }),
      listen("menu-closed", () => {
        setMenu(null);
        setPlacement(null);
      }),
    ]);
  }, [surface]);

  useEffect(() => {
    const onContextMenu = (event: MouseEvent) => {
      const path = event.composedPath();
      // Text fields keep the native menu — it is the only mouse-driven
      // cut/copy/paste the bar has.
      if (isEditableTarget(path)) return;
      event.preventDefault();
      // A right-click that happened inside a floating surface keeps it open:
      // closing the flyout under the menu leaves it hanging over nothing.
      const keepFlyout = path.some(
        (hop) =>
          hop instanceof Element && hop.closest(".surface-flyout") !== null,
      );
      for (const target of resolveContextTargets(path)) {
        const items = itemsFor(target);
        if (items !== null && items.length > 0) {
          void openContextMenuSurface(
            items,
            anchorPoint(event),
            keepFlyout,
          ).catch(reportError);
          return;
        }
      }
      void closeContextMenuSurface().catch(reportError);
    };
    document.addEventListener("contextmenu", onContextMenu);
    return () => {
      document.removeEventListener("contextmenu", onContextMenu);
    };
  }, []);

  if (surface !== "overlay" || menu === null) return null;
  return <OpenContextMenu menu={menu} placement={placement} />;
}

function OpenContextMenu({
  menu,
  placement,
}: {
  menu: MenuRequest;
  placement: MenuPlacement | null;
}) {
  const close = useCallback(() => {
    void closeContextMenuSurface(menu.generation).catch(reportError);
  }, [menu.generation]);
  // Stable per open menu: the panel measures itself when the anchor changes.
  const anchor = useMemo<MenuAnchor>(
    () => ({
      kind: "point",
      x: placement?.anchorX ?? 0,
      y: placement?.anchorY ?? 0,
    }),
    [placement],
  );
  const reportMeasure = useCallback(
    (size: { width: number; height: number }) => {
      if (placement !== null) return;
      void reportContextMenuMeasure({
        generation: menu.generation,
        width: Math.ceil(size.width),
        height: Math.ceil(size.height),
        inset: 8,
      }).catch(reportError);
    },
    [menu.generation, placement],
  );

  return (
    <MenuPanel
      items={menu.items}
      label={t("menu.label")}
      anchor={anchor}
      onClose={close}
      onCloseAll={close}
      onMeasure={reportMeasure}
    />
  );
}
