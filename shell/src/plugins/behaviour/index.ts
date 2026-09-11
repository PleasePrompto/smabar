/**
 * The plugin UI kit's behaviour layer.
 *
 * Importing this module registers every component's delegated listeners;
 * {@link installKitBehaviour} then binds them once for the whole window.
 * Adding a component means adding a module here — no wiring anywhere else.
 */
import "./controls";
import "./tables";
import "./menus";
import "./dialogs";
import "./native";
import "./choice";
import "./combobox";
import "./select";
import "./dates";
import "./temporal";
import "./countdown";
import "./tween";

export { installKitBehaviour, syncKit } from "./delegate";

/**
 * Kit markup that acts on its own click.
 *
 * A click that bubbles out of a flyout closes it, so anything the behaviour
 * layer drives has to stop there — otherwise pressing a copy button or
 * sorting a table would shut the flyout it lives in. This is the same
 * problem `handlesItsOwnClick` solves for native `<summary>` and
 * `[popovertarget]`, and it is checked the same way.
 */
export const KIT_INTERACTIVE = [
  "[data-sb-copy]",
  "[data-sb-copy-text]",
  "[data-sb-number-up]",
  "[data-sb-number-down]",
  "[data-sb-password-toggle]",
  "[data-sb-dropdown-toggle]",
  "[data-sb-context]",
  "[data-sb-multiselect]",
  "[data-sb-taginput]",
  "[data-sb-otp]",
  "[data-sb-combobox]",
  ".sb-temporal",
  "[data-sb-datepicker]",
  "[data-sb-daterange]",
  "[data-sb-datepicker-toggle]",
  "[data-sb-lightbox] a[href]",
  "th[data-sb-sort]",
  '[role="menuitem"]',
  ".sb-command__item",
  ".sb-lightbox__nav",
  ".sb-lightbox__close",
  ".sb-select-trigger",
  ".sb-select-option",
].join(", ");
