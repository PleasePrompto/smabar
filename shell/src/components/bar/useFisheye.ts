import { useEffect, type RefObject } from "react";
import { magnifyScale, type EffectsConfig } from "../../store/bar";
import { fisheyeScale } from "./metrics";
import { nativePointerIsInside, subscribePointerSamples } from "./useAutohide";

/**
 * Apple-dock fisheye magnify for the shortcut zone: ONE rAF-throttled
 * mousemove listener sets a `--magnify` CSS var per tile from its distance
 * to the cursor; bar.css turns that into a pure transform on the WHOLE tile
 * (icon + label — no reflow, see `.shortcut-tile`). A disabled effect
 * attaches no listener.
 *
 * The listener sits on the document and decides from the pointer
 * COORDINATES, not from the zone's hover state: the zone's box is inflated
 * by `reserve` px on all four sides so magnified tiles are not clipped, and
 * the vertical headroom lies outside the bar row AND outside the X11 input
 * shape. A pointer that leaves the bar through that band stops delivering
 * events while the DOM still hovers the zone, so a mouseleave-based reset
 * never fires and the tiles stay magnified. Only the row band counts as
 * hovered — hence the inset on every side.
 *
 * The same event drought follows a LAUNCH: the started window covers the
 * bar under a motionless pointer, so the last hover state would stay frozen
 * on screen. Hence the click reset below — moving the pointer over the bar
 * again re-magnifies from the next coordinates.
 */
export function useFisheye(
  ref: RefObject<HTMLElement | null>,
  effects: EffectsConfig,
  reserve: number,
): void {
  const scale = magnifyScale(effects);
  const neighbors = effects.hoverMagnify.neighbors;
  useEffect(() => {
    const zone = ref.current;
    if (!zone || scale <= 1) return;
    const tiles = () => zone.querySelectorAll<HTMLElement>(".shortcut-tile");
    let frame = 0;
    let magnified = false;
    let x = 0;
    let y = 0;

    // Cheap no-op while nothing is magnified: the pointer moving anywhere on
    // screen runs this, the zone is hovered for a fraction of that.
    const reset = () => {
      magnified = false;
      for (const tile of tiles()) tile.style.removeProperty("--magnify");
    };
    const pointerInZone = () => {
      const box = zone.getBoundingClientRect();
      return (
        x >= box.left + reserve &&
        x <= box.right - reserve &&
        y >= box.top + reserve &&
        y <= box.bottom - reserve
      );
    };
    const apply = () => {
      frame = 0;
      if (!nativePointerIsInside()) {
        if (magnified) reset();
        return;
      }
      // A running reorder drag owns the tile transforms (useDragReorder):
      // magnifying underneath it would fight the dragged tile and change the
      // very rects the drop position is measured against.
      if (zone.hasAttribute("data-reordering")) {
        if (magnified) reset();
        return;
      }
      if (!pointerInZone()) {
        if (magnified) reset();
        return;
      }
      magnified = true;
      const updates: [HTMLElement, number][] = [];
      for (const tile of tiles()) {
        const rect = tile.getBoundingClientRect();
        const distance = x - (rect.left + rect.width / 2);
        const value = fisheyeScale(distance, rect.width, scale, neighbors);
        updates.push([tile, value]);
      }
      // Keep every geometry read ahead of every style write: alternating
      // them makes the next getBoundingClientRect recalculate styles.
      for (const [tile, value] of updates) {
        tile.style.setProperty("--magnify", value.toFixed(3));
      }
    };
    const onMove = (event: MouseEvent) => {
      if (!nativePointerIsInside()) return;
      x = event.clientX;
      y = event.clientY;
      if (frame === 0) frame = requestAnimationFrame(apply);
    };
    // The pointer left the document (under the input shape: the window) —
    // no further coordinates arrive, so drop the magnify right here.
    const onOut = (event: MouseEvent) => {
      if (event.relatedTarget === null && magnified) reset();
    };
    // Capture phase: the tiles stop click propagation (tile tiles etc.),
    // so a bubbling listener would never see the launch click.
    const onClick = () => {
      if (magnified) reset();
    };

    document.addEventListener("mousemove", onMove, { passive: true });
    document.addEventListener("mouseout", onOut, { passive: true });
    zone.addEventListener("click", onClick, { capture: true, passive: true });
    const unsubscribePointerSamples = subscribePointerSamples(
      (sampleX, sampleY) => {
        if (Number.isFinite(sampleX) && Number.isFinite(sampleY)) {
          x = sampleX;
          y = sampleY;
          if ((magnified || frame !== 0) && !pointerInZone()) {
            if (frame !== 0) cancelAnimationFrame(frame);
            frame = 0;
            if (magnified) reset();
          } else if (pointerInZone() && frame === 0) {
            frame = requestAnimationFrame(apply);
          }
          return;
        }
        if (frame !== 0) cancelAnimationFrame(frame);
        frame = 0;
        if (magnified) reset();
      },
    );
    return () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseout", onOut);
      zone.removeEventListener("click", onClick, { capture: true });
      unsubscribePointerSamples();
      if (frame !== 0) cancelAnimationFrame(frame);
      reset();
    };
  }, [ref, scale, neighbors, reserve]);
}
