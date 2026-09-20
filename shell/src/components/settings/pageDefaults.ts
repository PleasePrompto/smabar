import {
  BAR_DEFAULTS,
  DESIGN_DEFAULTS,
  PLUGINS_DEFAULTS,
  SHORTCUTS_DEFAULTS,
  SYSTEM_DEFAULTS,
} from "./defaults";
import type { ConfigWrite } from "./persist";

/** Each visible page owns its reset; moving a control moves its default too. */
export function pageDefaults(page: string): readonly ConfigWrite[] {
  const all: readonly ConfigWrite[] = [
    ...BAR_DEFAULTS,
    ...DESIGN_DEFAULTS,
    ...PLUGINS_DEFAULTS,
    ...SHORTCUTS_DEFAULTS,
    ...SYSTEM_DEFAULTS,
  ];
  return all.filter(({ path }) => {
    if (page === "notifications") return path.startsWith("popups.");
    if (page === "themes") return path === "theme";
    if (page === "general") return path === "language";
    if (page === "advanced")
      return path.startsWith("mcp.") || path === "rendering";
    const behavior =
      path.startsWith("effects.") ||
      ["layout.behavior", "layout.yieldToFullscreen", "zOrder"].includes(path);
    if (page === "behavior") return behavior;
    if (page === "layout") return path.startsWith("layout.") && !behavior;
    if (page === "appearance")
      return (
        path.startsWith("appearance.") ||
        (path.startsWith("shortcuts.") && path !== "shortcuts.pinned")
      );
    return false;
  });
}
