import { getCurrentWindow } from "@tauri-apps/api/window";

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
    </>
  );
}
