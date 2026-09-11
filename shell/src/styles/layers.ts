/**
 * Z-index layers of the shell. Deliberately NOT theme tokens: stacking is
 * shell architecture, not a design decision plugins or themes may override.
 */

/** The bar strip itself. */
export const BAR = "var(--sb-z-bar)";
/** The solo variant's on-demand secondary zone row. */
/** Floating panels above everything (demo panel, settings). */
export const PANEL = "var(--sb-z-panel)";
/** Insertion marker of a running reorder drag — visible over every surface. */
export const DRAG_MARKER = "var(--sb-z-drag-marker)";
/** The context menu (and its one submenu level). */
export const CONTEXT_MENU = "var(--sb-z-context-menu)";
/** The tooltip — never interactive, always the topmost surface. */
export const TOOLTIP = "var(--sb-z-tooltip)";
