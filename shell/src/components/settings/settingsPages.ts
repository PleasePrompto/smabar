import type { StoreKind } from "../../ipc/store";

export const SETTINGS_GROUPS = [
  "bar",
  "shortcuts",
  "plugins",
  "system",
] as const;
export interface SettingsPage {
  id: string;
  group: string;
  labelKey: string;
  kind?: StoreKind;
}
export const SETTINGS_PAGES: readonly SettingsPage[] = [
  ...["layout", "behavior", "themes", "colors", "appearance"].map((page) => ({
    id: `bar/${page}`,
    group: "bar",
    labelKey: `settings.page.${page}`,
  })),
  {
    id: "bar/community",
    group: "bar",
    kind: "theme",
    labelKey: "settings.themes.communityTitle",
  },
  { id: "shortcuts", group: "shortcuts", labelKey: "settings.group.shortcuts" },
  { id: "plugins", group: "plugins", labelKey: "settings.page.installed" },
  {
    id: "plugins/store",
    group: "plugins",
    kind: "plugin",
    labelKey: "settings.store.title",
  },
  ...["general", "audio", "advanced", "about", "legal"].map((page) => ({
    id: `system/${page}`,
    group: "system",
    labelKey: `settings.page.${page}`,
  })),
];
const ALIASES: Readonly<Record<string, string>> = {
  bar: "bar/layout",
  design: "bar/themes",
  "design/themes": "bar/community",
  system: "system/general",
  "system/updates": "system/about",
  legal: "system/legal",
};
export function resolveSettingsPage(id: string): string {
  const resolved = ALIASES[id] ?? id;
  return SETTINGS_PAGES.some((page) => page.id === resolved) ||
    /^plugins\/detail\/[a-z0-9-]+$/.test(resolved)
    ? resolved
    : "bar/layout";
}
export function groupOf(id: string): string {
  return resolveSettingsPage(id).split("/")[0] ?? "bar";
}
export function pageOf(id: string): SettingsPage | null {
  return (
    SETTINGS_PAGES.find((page) => page.id === resolveSettingsPage(id)) ?? null
  );
}
export function pagesOf(group: string): SettingsPage[] {
  return SETTINGS_PAGES.filter((page) => page.group === group);
}
