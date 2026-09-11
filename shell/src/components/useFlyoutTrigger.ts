import {
  useCallback,
  useEffect,
  useRef,
  type MouseEvent,
  type RefObject,
} from "react";

import {
  nativePointerIsInside,
  pointerInAutohideRegion,
  subscribePointerSamples,
} from "./bar/useAutohide";
import { clampHoverPeekDelay, useSmabar, type FlyoutId } from "../store/bar";
import {
  closeFlyoutSurface,
  openFlyoutSurface,
  subscribeOverlayPointer,
} from "../ipc/overlay";
import { reportError } from "../ipc/log";

/** Lets the pointer cross the small tile-to-flyout gap without flicker. */
const PEEK_LEAVE_GRACE_MS = 160;

interface FlyoutTriggerOptions {
  /** Opens the hover preview even while the generic hover-peek effect is
   *  disabled — for tiles with dedicated hover content, whose presence
   *  overrides the global setting (the delay still applies). */
  forcePeek?: boolean;
}

/**
 * Click handler + active state for a tile's flyout trigger.
 * Pass a ref when the measured element differs from the clicked one.
 */
export function useFlyoutTrigger(
  id: FlyoutId,
  { forcePeek = false }: FlyoutTriggerOptions = {},
) {
  const toggleFlyout = useSmabar((s) => s.toggleFlyout);
  const isActive = useSmabar((s) => s.openFlyout === id);
  const hoverPeek = useSmabar((s) => s.effects.hoverPeek);
  const ref = useRef<HTMLButtonElement | null>(null);
  const openTimer = useRef(0);
  const closeTimer = useRef(0);
  /**
   * The overlay reported the pointer inside the flyout. A native leave of
   * the bar that follows is the pointer crossing INTO the flyout, not away
   * from it — the X11 pointer watchdog reports the bar window's edge a beat
   * after the overlay's own enter.
   */
  const overlayInside = useRef(false);

  const cancelOpen = useCallback(() => {
    window.clearTimeout(openTimer.current);
    openTimer.current = 0;
  }, []);
  const cancelClose = useCallback(() => {
    window.clearTimeout(closeTimer.current);
    closeTimer.current = 0;
  }, []);

  const trigger = (event: MouseEvent, ref?: RefObject<HTMLElement | null>) => {
    cancelOpen();
    cancelClose();
    event.stopPropagation();
    const el = ref?.current ?? event.currentTarget;
    const rect = el.getBoundingClientRect();
    const before = useSmabar.getState();
    toggleFlyout(id, rect);
    const after = useSmabar.getState();
    if (before.openFlyout === id && before.flyoutMode === "pinned") {
      void closeFlyoutSurface().catch(reportError);
    } else if (after.openFlyout === id && after.flyoutMode === "pinned") {
      void openFlyoutSurface(id, "pinned", rect).catch((error: unknown) => {
        useSmabar.getState().closeFlyout();
        reportError(error);
      });
    }
  };

  const queuePeek = useCallback(
    (el: Element) => {
      if (!nativePointerIsInside() || useSmabar.getState().reordering) return;
      cancelClose();
      const { openFlyout, flyoutMode } = useSmabar.getState();
      if ((!hoverPeek.enabled && !forcePeek) || flyoutMode === "pinned") return;
      if (openFlyout === id && flyoutMode === "peek") return;
      cancelOpen();
      openTimer.current = window.setTimeout(() => {
        const rect = el.getBoundingClientRect();
        overlayInside.current = false;
        useSmabar.getState().peekFlyout(id, rect, forcePeek);
        const state = useSmabar.getState();
        if (state.openFlyout === id && state.flyoutMode === "peek") {
          void openFlyoutSurface(id, "peek", rect).catch((error: unknown) => {
            useSmabar.getState().closePeek(id);
            reportError(error);
          });
        }
        openTimer.current = 0;
      }, clampHoverPeekDelay(hoverPeek.delayMs));
    },
    [
      cancelClose,
      cancelOpen,
      forcePeek,
      hoverPeek.delayMs,
      hoverPeek.enabled,
      id,
    ],
  );

  const peekEnter = (
    event: MouseEvent,
    ref?: RefObject<HTMLElement | null>,
  ) => {
    queuePeek(ref?.current ?? event.currentTarget);
  };

  const peekLeave = useCallback(() => {
    cancelOpen();
    cancelClose();
    closeTimer.current = window.setTimeout(() => {
      const state = useSmabar.getState();
      if (state.openFlyout === id && state.flyoutMode === "peek") {
        state.closePeek(id);
        void closeFlyoutSurface().catch(reportError);
      }
      closeTimer.current = 0;
    }, PEEK_LEAVE_GRACE_MS);
  }, [cancelClose, cancelOpen, id]);

  useEffect(() => {
    // Cancel synchronously: a short drag can finish before an old hover
    // timer fires, so checking only the flag inside that timer is too late.
    const unsubscribeDrag = useSmabar.subscribe((state, previous) => {
      if (state.reordering && !previous.reordering) {
        cancelOpen();
        cancelClose();
        overlayInside.current = false;
      }
    });
    const unsubscribeOverlayPointer = subscribeOverlayPointer((inside) => {
      overlayInside.current = inside;
      if (inside && useSmabar.getState().openFlyout === id) cancelClose();
      else if (!inside && useSmabar.getState().flyoutMode === "peek") {
        peekLeave();
      }
    });
    const unsubscribe = subscribePointerSamples((x, y) => {
      const { openFlyout, flyoutMode } = useSmabar.getState();
      if (Number.isFinite(x) && Number.isFinite(y)) {
        // WebKit on macOS filters DOM motion while another app is active.
        // The same native samples that clean up hover also initiate it.
        const element = ref.current;
        if (element !== null) {
          if (
            pointerInAutohideRegion(x, y, [element.getBoundingClientRect()])
          ) {
            cancelClose();
            if (openTimer.current === 0) queuePeek(element);
          } else if (
            openTimer.current !== 0 ||
            (openFlyout === id && flyoutMode === "peek")
          ) {
            if (!overlayInside.current && closeTimer.current === 0) peekLeave();
          }
        }
        return;
      }
      if (overlayInside.current) return;
      if (
        openTimer.current !== 0 ||
        (openFlyout === id && flyoutMode === "peek")
      ) {
        peekLeave();
      }
    });
    return () => {
      unsubscribeDrag();
      unsubscribe();
      unsubscribeOverlayPointer();
      cancelOpen();
      cancelClose();
    };
  }, [cancelClose, cancelOpen, id, peekLeave, queuePeek]);

  return {
    ref,
    trigger,
    isActive,
    peekEnter,
    peekLeave,
  };
}
