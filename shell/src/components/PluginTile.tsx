import { useRef, type MouseEvent, type ReactNode, type RefObject } from "react";

import { useSmabar, type TileChrome } from "../store/bar";

export function tileChrome(
  globalChrome: TileChrome,
  override?: TileChrome,
): TileChrome {
  return override ?? globalChrome;
}

interface PluginTileProps {
  triggerRef?: RefObject<HTMLButtonElement | null>;
  children: ReactNode;
  /** Namespaced registry id; the context menu resolves the tile through it. */
  tileId: string;
  /** Accessible name of the live zone. */
  label: string;
  /** Usually `useFlyoutTrigger(...).trigger`; receives the tile's own ref. */
  onTrigger: (
    event: MouseEvent<HTMLButtonElement>,
    ref: RefObject<HTMLButtonElement | null>,
  ) => void;
  isActive: boolean;
  hasFlyout?: boolean;
  /** Manifest override; absent follows appearance.tileChrome. */
  chrome?: TileChrome;
  onPeekEnter?: (
    event: MouseEvent<HTMLButtonElement>,
    ref: RefObject<HTMLButtonElement | null>,
  ) => void;
  onPeekLeave?: () => void;
  /** Inner layout classes (flex direction, gaps, …). */
  className?: string;
}

/** The clickable live zone of a tile: a card tile with hover feedback. */
export function PluginTile({
  triggerRef,
  children,
  tileId,
  label,
  onTrigger,
  isActive,
  hasFlyout = false,
  chrome,
  onPeekEnter,
  onPeekLeave,
  className = "flex items-center justify-center",
}: PluginTileProps) {
  const ownRef = useRef<HTMLButtonElement>(null);
  const ref = triggerRef ?? ownRef;
  const globalChrome = useSmabar((s) => s.appearance.tileChrome);
  const effectiveChrome = tileChrome(globalChrome, chrome);
  const card = effectiveChrome === "card";
  return (
    <button
      ref={ref}
      onClick={(e) => {
        onTrigger(e, ref);
      }}
      onMouseEnter={(event) => {
        onPeekEnter?.(event, ref);
      }}
      onMouseLeave={onPeekLeave}
      className={`plugin-tile plugin-lift rounded-sb-m ${className} ${
        card ? "surface-tile surface-tile-hover" : ""
      } ${isActive ? "surface-tile-active" : ""}`}
      data-tile-chrome={effectiveChrome}
      data-tile-id={tileId}
      aria-label={label}
      aria-expanded={hasFlyout ? isActive : undefined}
    >
      {children}
    </button>
  );
}
