/**
 * The tooltip controller, replacing the webview's native (GTK) tooltip,
 * which renders in the system palette and ignores the bar's theme entirely.
 *
 * Declarative: any element carrying `data-sb-tooltip="text"` gets one.
 * Plugin markup declares plain `title` instead; the ShadowHost enhancer
 * moves it onto this attribute (tiles/decorators.ts), which is also what
 * makes the native tooltip disappear.
 *
 * Two presentations, one controller. The bar hands the text to the
 * dedicated overlay window (`openTooltipSurface`). The overlay window
 * itself — an open flyout — presents INLINE: the overlay is the one window
 * shared by flyout, menu and tooltip, so the core refuses a second surface
 * while a flyout is up, and a tooltip inside a flyout is drawn in the
 * flyout's own document instead, clamped to the window.
 *
 * The portaled surface is purely visual (pointer-events: none, aria-hidden):
 * plugin title text stays on the trigger as its accessible name or as a
 * persistent hidden description, so screen readers never depend on this
 * transient layer. That deliberately trades WCAG 1.4.13's
 * "hoverable" clause (a pointer cannot rest ON the tooltip) for a hard
 * platform constraint: an interactive tooltip would have to join the X11
 * input shape and would then eat desktop clicks around the bar. Nothing is
 * lost — the text is the trigger's accessible name, it stays up as long as
 * the pointer rests on the trigger, and Escape dismisses it.
 */

import { useEffect, useLayoutEffect, useRef, useState } from "react";

import {
  nativePointerIsInside,
  subscribePointerSamples,
} from "../bar/useAutohide";
import { isRenderableRect, type FlyoutRect } from "../../store/bar";
import { closeTooltipSurface, openTooltipSurface } from "../../ipc/overlay";
import { reportError } from "../../ipc/log";
import { TOOLTIP } from "../../styles/layers";

/** Attribute that turns any element into a tooltip trigger. */
export const TOOLTIP_ATTR = "data-sb-tooltip";
/**
 * Hover dwell before the tooltip opens. 400 ms is the usual desktop range
 * (long enough that crossing a dock does not flash a dozen tooltips, short
 * enough to feel immediate when the pointer rests).
 */
export const TOOLTIP_DELAY_MS = 400;
/**
 * How often a visible tooltip re-checks its anchor. The input shape can
 * swallow every pointer event once the pointer leaves it (the documented
 * X11 trap), so liveness cannot rely on pointerout alone.
 */
export const TOOLTIP_WATCHDOG_MS = 250;
/** Edge inset and trigger gap of an inline tooltip, in CSS pixels. */
const INLINE_INSET = 8;
const INLINE_GAP = 8;

interface TooltipState {
  text: string;
  rect: FlyoutRect;
}

export interface InlinePlacement {
  side: "top" | "bottom";
  x: number;
  y: number;
}

/**
 * Places an inline tooltip beside its trigger inside the window: above when
 * there is room, otherwise below; centred, clamped to the viewport inset.
 */
export function placeInline(
  trigger: FlyoutRect,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
): InlinePlacement {
  const x = Math.min(
    Math.max(trigger.left + trigger.width / 2 - size.width / 2, INLINE_INSET),
    Math.max(viewport.width - size.width - INLINE_INSET, INLINE_INSET),
  );
  const above = trigger.top - INLINE_GAP - size.height;
  if (above >= INLINE_INSET) return { side: "top", x, y: above };
  const below = trigger.top + trigger.height + INLINE_GAP;
  return {
    side: "bottom",
    x,
    y: Math.max(
      Math.min(below, viewport.height - size.height - INLINE_INSET),
      INLINE_INSET,
    ),
  };
}

/** Innermost tooltip trigger in a composed event path. */
function findTrigger(path: readonly EventTarget[]): HTMLElement | null {
  if (
    path.some(
      (hop) =>
        hop instanceof HTMLElement && hop.hasAttribute("data-hover-flyout"),
    )
  ) {
    return null;
  }
  for (const hop of path) {
    if (!(hop instanceof HTMLElement)) continue;
    if (hop.hasAttribute(TOOLTIP_ATTR)) return hop;
  }
  return null;
}

function rectOf(element: HTMLElement): FlyoutRect {
  const rect = element.getBoundingClientRect();
  return {
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height,
  };
}

function inside(rect: FlyoutRect, x: number, y: number): boolean {
  return (
    x >= rect.left &&
    x <= rect.left + rect.width &&
    y >= rect.top &&
    y <= rect.top + rect.height
  );
}

function sameRect(left: FlyoutRect, right: FlyoutRect): boolean {
  return (
    left.left === right.left &&
    left.top === right.top &&
    left.width === right.width &&
    left.height === right.height
  );
}

export function TooltipLayer({ inline = false }: { inline?: boolean }) {
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);

  useEffect(() => {
    if (inline) return;
    const operation =
      tooltip === null
        ? closeTooltipSurface()
        : openTooltipSurface(tooltip.text, tooltip.rect);
    void operation.catch(reportError);
  }, [tooltip, inline]);

  useEffect(() => {
    let timer = 0;
    let leaveTimer = 0;
    let watchdog = 0;
    let visible = false;
    /** Element currently scheduled or shown, from pointer or keyboard. */
    let anchor: HTMLElement | null = null;
    let pointerAnchor: HTMLElement | null = null;
    let focusAnchor: HTMLElement | null = null;

    const stopWatchdog = () => {
      window.clearInterval(watchdog);
      watchdog = 0;
    };

    const cancel = () => {
      window.clearTimeout(timer);
      window.clearTimeout(leaveTimer);
      timer = 0;
      leaveTimer = 0;
      visible = false;
      anchor = null;
      pointerAnchor = null;
      focusAnchor = null;
      stopWatchdog();
      setTooltip(null);
    };

    const hide = () => {
      visible = false;
      stopWatchdog();
      setTooltip(null);
    };

    // Liveness backstop while a tooltip is showing: an anchor that
    // unmounted, collapsed to a zero rect, or left the viewport (an
    // autohidden bar retracting) closes the tooltip even when no pointer
    // event ever arrives. One getBoundingClientRect per tick — far below
    // what the fisheye already does per frame.
    const startWatchdog = () => {
      if (watchdog !== 0) return;
      watchdog = window.setInterval(() => {
        if (anchor === null) {
          cancel();
          return;
        }
        const rect = rectOf(anchor);
        if (
          !anchor.isConnected ||
          !isRenderableRect(rect, window.innerWidth, window.innerHeight)
        ) {
          cancel();
          return;
        }
        const text = anchor.getAttribute(TOOLTIP_ATTR) ?? "";
        if (text.trim() === "") {
          cancel();
          return;
        }
        setTooltip((current) =>
          current !== null &&
          current.text === text &&
          sameRect(current.rect, rect)
            ? current
            : { text, rect },
        );
      }, TOOLTIP_WATCHDOG_MS);
    };

    const syncAnchor = () => {
      const next = pointerAnchor ?? focusAnchor;
      if (next === anchor) return;
      window.clearTimeout(timer);
      anchor = next;
      if (next === null) {
        hide();
        return;
      }
      const text = next.getAttribute(TOOLTIP_ATTR) ?? "";
      if (text.trim() === "") {
        cancel();
        return;
      }
      if (visible) {
        const rect = rectOf(next);
        if (!isRenderableRect(rect, window.innerWidth, window.innerHeight)) {
          // Switching to an anchor that is hidden or already detached must
          // clear the tooltip, never re-anchor it at a zero rect.
          cancel();
          return;
        }
        setTooltip({ text, rect });
        startWatchdog();
        return;
      }
      setTooltip(null);
      timer = window.setTimeout(() => {
        timer = 0;
        // The anchor may have died during the dwell; this late measurement
        // is the moment that used to capture a zero rect and clamp the
        // tooltip into the top-left corner.
        const rect = rectOf(next);
        if (!isRenderableRect(rect, window.innerWidth, window.innerHeight)) {
          cancel();
          return;
        }
        visible = true;
        setTooltip({ text, rect });
        startWatchdog();
      }, TOOLTIP_DELAY_MS);
    };

    const onOver = (event: PointerEvent) => {
      if (!nativePointerIsInside()) return;
      window.clearTimeout(leaveTimer);
      leaveTimer = 0;
      pointerAnchor = findTrigger(event.composedPath());
      syncAnchor();
    };

    // Moving between elements INSIDE a plugin's shadow root dispatches no
    // pointerover outside it (target and relatedTarget both retarget to the
    // host, so the event stops at the boundary): a trigger reached from
    // within its plugin's markup is discovered by the move itself.
    //
    // Leaving is decided from pointer COORDINATES, never from DOM hover: a
    // magnified dock tile reaches past the X11 input shape, so leaving it
    // upwards delivers no pointerout and the tooltip would hang forever
    // (same rule as useFisheye / useAutohide).
    const onMove = (event: PointerEvent) => {
      if (!nativePointerIsInside()) return;
      const next = findTrigger(event.composedPath());
      if (next !== null && next !== pointerAnchor) {
        window.clearTimeout(leaveTimer);
        leaveTimer = 0;
        pointerAnchor = next;
        syncAnchor();
        return;
      }
      if (pointerAnchor === null) return;
      if (!inside(rectOf(pointerAnchor), event.clientX, event.clientY)) {
        pointerAnchor = null;
        syncAnchor();
      }
    };

    // The pointer left the window (under the input shape: the shaped
    // region) — no further coordinates arrive.
    const onOut = (event: PointerEvent) => {
      if (event.relatedTarget === null) {
        // Replacing hovered plugin markup emits out/over as one transition.
        // Delay the real window-leave by one task so that transition stays live.
        leaveTimer = window.setTimeout(() => {
          leaveTimer = 0;
          pointerAnchor = null;
          syncAnchor();
        }, 0);
      }
    };

    const onFocusIn = (event: FocusEvent) => {
      focusAnchor = findTrigger(event.composedPath());
      syncAnchor();
    };

    const onFocusOut = (event: FocusEvent) => {
      const trigger = findTrigger(event.composedPath());
      if (trigger === null || trigger !== focusAnchor) return;
      const next = event.relatedTarget;
      if (next instanceof Node && trigger.contains(next)) return;
      focusAnchor = null;
      syncAnchor();
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") cancel();
    };

    document.addEventListener("pointerover", onOver, { passive: true });
    document.addEventListener("pointermove", onMove, { passive: true });
    document.addEventListener("pointerout", onOut, { passive: true });
    document.addEventListener("pointerdown", cancel, { passive: true });
    document.addEventListener("focusin", onFocusIn);
    document.addEventListener("focusout", onFocusOut);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", cancel);
    window.addEventListener("resize", cancel);
    const unsubscribePointerSamples = subscribePointerSamples((x, y) => {
      if (Number.isFinite(x) && Number.isFinite(y)) {
        if (pointerAnchor !== null && !inside(rectOf(pointerAnchor), x, y)) {
          pointerAnchor = null;
          syncAnchor();
        }
        return;
      }
      pointerAnchor = null;
      syncAnchor();
    });
    return () => {
      window.clearTimeout(timer);
      window.clearTimeout(leaveTimer);
      stopWatchdog();
      document.removeEventListener("pointerover", onOver);
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerout", onOut);
      document.removeEventListener("pointerdown", cancel);
      document.removeEventListener("focusin", onFocusIn);
      document.removeEventListener("focusout", onFocusOut);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("resize", cancel);
      unsubscribePointerSamples();
    };
  }, []);

  if (!inline || tooltip === null) return null;
  return <InlineTooltip text={tooltip.text} rect={tooltip.rect} />;
}

function InlineTooltip({ text, rect }: TooltipState) {
  const ref = useRef<HTMLDivElement>(null);
  const [placement, setPlacement] = useState<InlinePlacement | null>(null);

  // Measured after paint, like the overlay window's tooltip: the size decides
  // the side, and the text decides the size.
  useLayoutEffect(() => {
    if (ref.current === null) return;
    const size = ref.current.getBoundingClientRect();
    setPlacement(
      placeInline(
        rect,
        { width: size.width, height: size.height },
        { width: window.innerWidth, height: window.innerHeight },
      ),
    );
  }, [text, rect]);

  return (
    <div
      ref={ref}
      className="overlay-tooltip fixed"
      aria-hidden="true"
      data-side={placement?.side}
      style={{
        zIndex: TOOLTIP,
        left: placement?.x ?? 0,
        top: placement?.y ?? 0,
        visibility: placement === null ? "hidden" : "visible",
      }}
    >
      {text}
    </div>
  );
}
