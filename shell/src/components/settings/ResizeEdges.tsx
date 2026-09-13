import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { t } from "../../i18n/t";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

import { reportError } from "../../ipc/log";

/**
 * The window's resize handles: invisible strips along every edge and
 * corner that hand the gesture to the window manager, the way a native
 * frame would. The names are Tauri's `ResizeDirection`.
 */
const DIRECTIONS = [
  "North",
  "South",
  "East",
  "West",
  "NorthEast",
  "NorthWest",
  "SouthEast",
  "SouthWest",
] as const;

export function ResizeEdges() {
  const resizeWithKeyboard = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    const step = event.shiftKey ? 64 : 24;
    const next = {
      width: window.innerWidth,
      height: window.innerHeight,
    };
    switch (event.key) {
      case "ArrowLeft":
        next.width -= step;
        break;
      case "ArrowRight":
        next.width += step;
        break;
      case "ArrowUp":
        next.height -= step;
        break;
      case "ArrowDown":
        next.height += step;
        break;
      default:
        return;
    }
    event.preventDefault();
    void getCurrentWindow()
      .setSize(
        new LogicalSize(Math.max(640, next.width), Math.max(480, next.height)),
      )
      .catch(reportError);
  };
  return (
    <>
      {DIRECTIONS.map((direction) => (
        <div
          key={direction}
          className="settings-resize-edge"
          data-direction={direction}
          aria-hidden="true"
          onMouseDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void getCurrentWindow()
              .startResizeDragging(direction)
              .catch(reportError);
          }}
        />
      ))}
      <button
        type="button"
        className="settings-resize-control"
        aria-label={t("settings.resize")}
        title={t("settings.resize")}
        onKeyDown={resizeWithKeyboard}
      />
    </>
  );
}
