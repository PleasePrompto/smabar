import type { StoreKind } from "../../ipc/store";

/**
 * A page a settings group opens beside its scrolling sections. The two
 * Community Store lists are pages: they replace the group's content and
 * carry their own back button, but stay listed under Plugins and Design.
 *
 * A page id is `<group>/<page>`; everything before the slash is the group
 * the navigation highlights.
 */
export interface SettingsPage {
  id: string;
  group: string;
  kind: StoreKind;
  /** The i18n key of the navigation label. */
  labelKey: string;
  /**
   * The section (by index) the entry is listed after; omitted, it follows
   * the last one. The Plugin Store belongs right under Installed.
   */
  after?: number;
}

export const SETTINGS_PAGES: readonly SettingsPage[] = [
  {
    id: "plugins/store",
    group: "plugins",
    kind: "plugin",
    labelKey: "settings.store.title",
    after: 0,
  },
  {
    id: "design/themes",
    group: "design",
    kind: "theme",
    labelKey: "settings.themes.communityTitle",
    after: 0,
  },
];

/** The group a settings id belongs to: `plugins/store` → `plugins`. */
export function groupOf(id: string): string {
  const slash = id.indexOf("/");
  return slash === -1 ? id : id.slice(0, slash);
}

/** The page an id names, or null for a plain group. */
export function pageOf(id: string): SettingsPage | null {
  return SETTINGS_PAGES.find((page) => page.id === id) ?? null;
}

/** The pages listed under one group. */
export function pagesOf(group: string): SettingsPage[] {
  return SETTINGS_PAGES.filter((page) => page.group === group);
}
