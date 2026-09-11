import { useCallback, useEffect, useRef, type RefObject } from "react";

import { useSmabar } from "../../store/bar";

/**
 * Horizontal overflow behavior for a bar zone: vertical wheel input scrolls
 * sideways, `data-zone-overflowing` turns the zone into a scroll container
 * only while its content actually overflows (a permanent scroll container
 * would clip hover/magnify effects at the row edge — see `.zone-scroll` in
 * bar.css), `data-fade-left`/`data-fade-right` drive the CSS edge fades,
 * and scrolling closes any open flyout (its trigger rect would drift away
 * from the tile).
 */
export function useZoneScroll(): RefObject<HTMLDivElement | null> {
  const ref = useRef<HTMLDivElement>(null);

  const updateFades = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    // Overflow from layout offsets, NOT scrollWidth: a hover-magnified tile
    // extends the scrollable overflow, and flipping into a scroll container
    // mid-effect would clip the very transform that caused the flip. The
    // offsets are relative to the zone itself (position: relative, bar.css).
    let end = 0;
    let hasContent = false;
    for (const child of el.children) {
      if (!(child instanceof HTMLElement)) continue;
      hasContent = true;
      end = Math.max(end, child.offsetLeft + child.offsetWidth);
    }
    if (hasContent) {
      end +=
        Number.parseFloat(getComputedStyle(el).paddingInlineEnd || "0") || 0;
    }
    // 1px slack: fractional scroll positions never quite reach the ends.
    el.toggleAttribute("data-zone-overflowing", end > el.clientWidth + 1);
    el.toggleAttribute("data-fade-left", el.scrollLeft > 1);
    el.toggleAttribute(
      "data-fade-right",
      el.scrollLeft + el.clientWidth < end - 1,
    );
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (el.scrollWidth <= el.clientWidth) return;
      const delta =
        Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      if (delta === 0) return;
      e.preventDefault();
      el.scrollLeft += delta;
    };
    const onScroll = () => {
      updateFades();
      const store = useSmabar.getState();
      if (store.openFlyout !== null) store.closeFlyout();
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    el.addEventListener("scroll", onScroll, { passive: true });
    const resize = new ResizeObserver(updateFades);
    const observeChildren = () => {
      resize.disconnect();
      resize.observe(el);
      for (const child of el.children) {
        if (child instanceof HTMLElement) resize.observe(child);
      }
    };
    observeChildren();
    const mutations = new MutationObserver(() => {
      observeChildren();
      updateFades();
    });
    mutations.observe(el, { childList: true });
    // Theme tokens change gaps/padding through :root without resizing or
    // re-rendering this zone. The browser batches the many style writes into
    // one callback.
    const themeMutations = new MutationObserver(updateFades);
    themeMutations.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["style"],
    });
    return () => {
      el.removeEventListener("wheel", onWheel);
      el.removeEventListener("scroll", onScroll);
      mutations.disconnect();
      themeMutations.disconnect();
      resize.disconnect();
    };
  }, [updateFades]);

  // No dependency array on purpose: content changes (tiles registering,
  // shortcuts refetched) alter scrollWidth without resizing the element.
  useEffect(updateFades);

  return ref;
}
