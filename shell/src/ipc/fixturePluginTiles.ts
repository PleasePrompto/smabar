import { registerTile, unregisterPluginTiles } from "../components/registry";
import { useSmabar } from "../store/bar";

/**
 * Browser-dev demo plugin tiles (no plugin process behind them): the clock,
 * a hover-vs-click weather tile, a carousel gallery, and a tabbed flyout —
 * plus a mixed popup stack (sticky + timed). Exercises the same sanitize/
 * enhance pipeline the real plugin renders go through.
 *
 * The clock mirrors what plugins/clock pushes, so browser-dev shows a real
 * clock even though no Python process runs here — and it keeps the two-line
 * tile and the analog face on the visual-check path.
 */

/** Inline SVG slide as a data:image/svg+xml URI (sanitizer-approved). */
function slide(label: string, from: string, to: string): string {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360">` +
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">` +
    `<stop offset="0" stop-color="${from}"/><stop offset="1" stop-color="${to}"/>` +
    `</linearGradient></defs>` +
    `<rect width="640" height="360" fill="url(#g)"/>` +
    `<text x="32" y="200" font-family="sans-serif" font-size="64" fill="white">${label}</text>` +
    `</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/** Zone-less: the shell's clock enhancer then uses the system time zone. */
const CLOCK_TILE =
  '<div class="sb-tile"><span class="sb-tile-stack">' +
  '<span class="sb-mono"><span data-clock-text="" data-clock-format="time"></span></span>' +
  '<span><span data-clock-text="" data-clock-format="date"></span></span>' +
  "</span></div>";

const CLOCK_HOVER =
  '<div class="sb-header"><span class="sb-title">System time</span></div>' +
  '<div class="sb-list">' +
  '<div class="sb-row"><span>Weekday</span>' +
  '<span><span data-clock-text="" data-clock-format="weekday"></span></span></div>' +
  '<div class="sb-row"><span>Week</span>' +
  '<span class="sb-mono"><span data-clock-text="" data-clock-format="week"></span></span></div>' +
  '<div class="sb-row"><span>UTC offset</span>' +
  '<span class="sb-mono"><span data-clock-text="" data-clock-format="offset"></span></span></div>' +
  "</div>";

/** Mirrors the clock plugin's search: sb-field + sb-reveal, no inline styles. */
const CLOCK_SEARCH =
  '<div class="sb-section">Add a world clock</div>' +
  '<div class="sb-field">' +
  '<input class="sb-input" data-field="query" placeholder="City or country, e.g. Tokyo">' +
  '<button class="sb-btn" data-action="search">Search</button>' +
  "</div>" +
  '<div class="sb-reveal sb-active"><div class="sb-list">' +
  ["Asia/Tokyo", "America/New_York", "Australia/Sydney"]
    .map(
      (zone) =>
        '<div class="sb-row"><span>' +
        (zone.split("/")[1]?.replace("_", " ") ?? zone) +
        `<br><small class="sb-faint">${zone}</small></span>` +
        '<button class="sb-btn sb-btn-icon sb-push" data-action="add" ' +
        `data-value="${zone}" aria-label="Add"><span data-lucide="plus"></span></button></div>`,
    )
    .join("") +
  "</div></div>";

const CLOCK_FLYOUT =
  '<div class="sb-header"><span class="sb-title">Clock</span></div>' +
  '<div class="sb-carousel" data-carousel>' +
  ["", "Asia/Tokyo", "America/New_York"]
    .map(
      (zone) =>
        '<div class="sb-card sb-center">' +
        `<div data-clock="${zone}"></div>` +
        '<div class="sb-mono sb-text-xl">' +
        `<span data-clock-text="${zone}" data-clock-format="time"></span></div>` +
        `<div class="sb-dim">${zone === "" ? "System time" : (zone.split("/")[1]?.replace("_", " ") ?? zone)} · ` +
        `<span data-clock-text="${zone}" data-clock-format="offset"></span></div>` +
        "</div>",
    )
    .join("") +
  "</div>" +
  CLOCK_SEARCH;

const WEATHER_HOVER =
  '<div class="sb-hero"><small>Berlin · now</small><h1>21°</h1>' +
  '<span class="sb-badge sb-badge-success">clearing up</span></div>' +
  '<div class="sb-tip"><span data-lucide="umbrella"></span>Rain expected after 18:00</div>';

const WEATHER_FLYOUT =
  '<div class="sb-header"><span class="sb-icon-badge"><span data-lucide="cloud-sun"></span></span>' +
  '<span class="sb-title">Weather settings</span></div>' +
  '<label>City</label><input class="sb-input" data-field="city" placeholder="Berlin">' +
  '<div class="sb-row"><label>Metric units</label>' +
  '<input class="sb-toggle" type="checkbox" data-field="metric" checked></div>' +
  '<button class="sb-btn sb-btn-primary" data-action="save">Save</button>';

const GALLERY_FLYOUT =
  '<div class="sb-header"><span class="sb-icon-badge"><span data-lucide="image"></span></span>' +
  '<span class="sb-title">Gallery</span></div>' +
  '<div class="sb-media" data-carousel data-carousel-interval="4000">' +
  `<img src="${slide("One", "#7c3aed", "#0ea5e9")}" alt="Slide one">` +
  `<img src="${slide("Two", "#0ea5e9", "#22c55e")}" alt="Slide two">` +
  `<img src="${slide("Three", "#ec4899", "#f59e0b")}" alt="Slide three">` +
  "</div>" +
  '<p class="sb-muted">Fresh photos — <a href="https://example.com/gallery">open the full gallery</a></p>';

/**
 * Demo of the plugin context-menu contract (ui-kit conventions.contextMenu):
 * icons, a separator, one submenu level with check marks, a destructive and
 * a disabled entry. Selecting one sends a normal plugin action, which the
 * browser-dev fixture accepts and drops.
 */
const GALLERY_MENU = JSON.stringify([
  {
    action: "download",
    value: "latest",
    label: "Download latest photo",
    icon: "download",
  },
  { separator: true },
  {
    label: "Sort by",
    icon: "list",
    items: [
      { action: "sort", value: "name", label: "Name", checked: true },
      { action: "sort", value: "date", label: "Date", checked: false },
      { action: "sort", value: "size", label: "Size", checked: false },
    ],
  },
  { action: "share", label: "Share album…", icon: "share-2" },
  { action: "sync", label: "Syncing…", icon: "refresh-cw", disabled: true },
  { action: "purge", label: "Empty album", icon: "trash-2", danger: true },
]);

const GALLERY_TILE =
  `<div class="sb-tile" title="Camera roll · 128 photos" data-context-items='${GALLERY_MENU}'>` +
  '<span data-lucide="image"></span>Gallery</div>';

const TABS_FLYOUT =
  '<div class="sb-header"><span class="sb-icon-badge"><span data-lucide="bitcoin"></span></span>' +
  '<span class="sb-title">Crypto</span></div>' +
  "<div data-tabs>" +
  '<div class="sb-tabs">' +
  '<button class="sb-tab" data-tab="price">Price</button>' +
  '<button class="sb-tab" data-tab="news">News</button>' +
  "</div>" +
  '<div data-tab-panel="price"><div class="sb-hero"><small>Bitcoin</small><h1>$64,120</h1>' +
  '<span class="sb-badge sb-badge-success">+2.4%</span></div></div>' +
  '<div data-tab-panel="news"><div class="sb-list">' +
  '<div class="sb-row"><span data-lucide="trending-up"></span><span>ETF inflows continue</span></div>' +
  '<div class="sb-row"><span data-lucide="globe"></span><span>Hashrate at record high</span></div>' +
  "</div></div></div>";

/** The manifest a real `demo` plugin would ship. */
export const DEMO_PLUGIN = {
  pluginId: "demo",
  name: "Demo",
  // Mirrors a real manifest's settingsSchema so the settings panel's
  // generic plugin form is exercised in browser-dev too.
  settingsSchema: {
    type: "object",
    properties: {
      label: {
        type: "string",
        description: "Location shown on the weather tile",
        default: "Berlin",
      },
      refreshMinutes: {
        type: "number",
        description: "How often the demo data is refreshed",
        default: 10,
      },
      showSeconds: {
        type: "boolean",
        description: "Show seconds on the clock tile",
        default: true,
      },
      units: {
        type: "string",
        enum: ["metric", "imperial"],
        description: "Unit system for the weather tile",
        default: "metric",
      },
      coins: {
        type: "array",
        items: { type: "string" },
        description: "Coins tracked by the crypto tile",
        default: ["bitcoin", "ethereum"],
      },
      zones: {
        type: "array",
        items: { type: "object" },
        description: "World clocks (edited in the clock tile itself)",
      },
    },
  },
  tiles: [
    { id: "clock", name: "Clock", hasFlyout: true },
    { id: "weather", name: "Weather", hasFlyout: true },
    { id: "gallery", name: "Gallery", hasFlyout: true },
    { id: "tabs", name: "Crypto", hasFlyout: true },
  ],
};

/**
 * Registers (or removes) the demo plugin's tiles. Deactivating a plugin
 * takes its tiles off the bar in the real app — browser-dev has to do the
 * same, or the settings panel would claim a plugin is off while its tiles are
 * still there.
 */
export function setDemoPluginRegistered(
  pluginId: string,
  registered: boolean,
): void {
  if (pluginId !== DEMO_PLUGIN.pluginId) return;
  const store = useSmabar.getState();
  if (!registered) {
    unregisterPluginTiles(pluginId);
    store.bumpRegistryVersion();
    return;
  }
  // The cached HTML in the store survives, so the tiles come back filled.
  DEMO_PLUGIN.tiles.forEach((tile) => {
    registerTile({
      id: `plugin:${pluginId}:${tile.id}`,
      pluginId,
      tile,
      meta: { name: tile.name },
    });
  });
  store.bumpRegistryVersion();
}

/** Seeds the demo tiles and the popup stack once (DevFixture). */
export function seedPluginDemos(): void {
  const store = useSmabar.getState();
  if (store.pluginUi["demo/weather/tile"] !== undefined) return;

  // The fixture registers tiles directly instead of going through the
  // bridge, so the settings panel only learns about the schema here.
  store.setPluginSchema(DEMO_PLUGIN.pluginId, {
    name: DEMO_PLUGIN.name,
    settingsSchema: DEMO_PLUGIN.settingsSchema,
  });
  setDemoPluginRegistered(DEMO_PLUGIN.pluginId, true);

  store.setPluginUi("demo/clock/tile", CLOCK_TILE);
  store.setPluginUi("demo/clock/hover", CLOCK_HOVER);
  store.setPluginUi("demo/clock/flyout", CLOCK_FLYOUT);
  // Two-line tile: exercises sb-tile-stack in browser-dev, where no plugin
  // process exists to push one.
  store.setPluginUi(
    "demo/weather/tile",
    '<div class="sb-tile"><span data-lucide="cloud-sun"></span>' +
      '<span class="sb-tile-stack"><span class="sb-mono">21°</span>' +
      "<span>Berlin</span></span></div>",
  );
  store.setPluginUi("demo/weather/hover", WEATHER_HOVER);
  store.setPluginUi("demo/weather/flyout", WEATHER_FLYOUT);
  store.setPluginUi("demo/gallery/tile", GALLERY_TILE);
  store.setPluginUi("demo/gallery/flyout", GALLERY_FLYOUT);
  store.setPluginUi(
    "demo/tabs/tile",
    '<div class="sb-tile"><span data-lucide="bitcoin"></span>$64k</div>',
  );
  store.setPluginUi("demo/tabs/flyout", TABS_FLYOUT);
  store.bumpRegistryVersion();

  const toasts: { html: string; ttlMs: number | null }[] = [
    {
      html:
        '<div class="sb-tile"><span data-lucide="download"></span><b>Download finished</b></div>' +
        '<p class="sb-muted">smabar-notes.pdf — <a href="https://example.com/files">open folder</a></p>',
      ttlMs: null,
    },
    {
      html:
        `<img class="sb-media sb-media-cover" src="${slide("New photo", "#0ea5e9", "#7c3aed")}" alt="New photo">` +
        '<p class="sb-muted">Camera roll synced</p>',
      ttlMs: null,
    },
    {
      html:
        '<div class="sb-tile"><span data-lucide="triangle-alert"></span><b>CPU spike</b></div>' +
        '<p class="sb-muted">cargo build is using 97% for 3 minutes</p>',
      ttlMs: 25_000,
    },
    {
      html:
        '<div class="sb-tile"><span data-lucide="calendar"></span><b>Meeting in 10 minutes</b></div>' +
        '<p class="sb-muted">Design review with Sarah</p>',
      ttlMs: 45_000,
    },
  ];
  toasts.forEach(({ html, ttlMs }) => {
    useSmabar.getState().enqueuePopup({
      pluginId: "demo",
      tileId: "toast",
      html,
      ttlMs,
    });
  });
}
