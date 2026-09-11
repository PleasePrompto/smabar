---
title: Privacy notice for the desktop app
description: Information under Art. 13 GDPR on the connections the smabar desktop application makes.
updated: 2026-09-11
---

## 1. Controller

The controller for the processing described here is:

Gérôme Dexheimer\
Wiener Straße 74\
48145 Münster\
Germany\
Email: hello@smabar.com

This notice covers the desktop application smabar. Visits to smabar.com are covered by the website's privacy policy at https://smabar.com/privacy/.

## 2. The connections the app makes

smabar runs on your computer, needs no account, shows no advertising and sends no usage or telemetry data. Connections are made only in the following cases. The requested server receives your IP address, the time and the technical details of the request.

- **Update check.** Direct installations (DEB, RPM, Windows installer, macOS DMG) request https://updates.smabar.com/latest.json about one minute after start and every six hours after that. The request carries no smabar version and no device identifier. An update is downloaded from the same address only after your confirmation. updates.smabar.com is delivered through Cloudflare, Inc., 101 Townsend St., San Francisco, CA 94107, USA. An installation from the Microsoft Store makes no such check; Microsoft delivers updates under its own privacy terms.
- **Community catalog.** About 90 seconds after start, every six hours after that and before every installation, smabar loads the signed catalog from https://store.smabar.com. The request carries the user agent `smabar/<version>` and the identifier of the catalog loaded last (ETag), so an unchanged catalog is not transferred again. When you open an entry, smabar loads its description from the same address. store.smabar.com runs on a server we manage; the connection passes through Cloudflare.
- **Bundled plugins that fetch data.** Three of the bundled plugins fetch data from third parties while they are active, without any further setting: the Weather plugin requests the forecast for the configured places (default: Berlin) from Open-Meteo (api.open-meteo.com) every 15 minutes and transmits the place's coordinates; when you add a place, the name you typed is sent to geocoding-api.open-meteo.com. The Clock plugin uses the same place search when you add a world clock. The Crypto plugin requests the prices of the configured coins (default: Bitcoin and Ethereum in euro) from CoinGecko (api.coingecko.com) every five minutes. These services receive your IP address; their privacy terms at https://open-meteo.com and https://www.coingecko.com apply. Deactivating a plugin under Settings › Plugins ends its process and stops the requests; merely hiding the plugin is not enough. The System info, Media and Tasks plugins work without a network connection.
- **Plugin and theme downloads.** For an installation you confirmed, smabar downloads the plugin archive or the theme file directly from GitHub (github.com, codeload.github.com, raw.githubusercontent.com) with the user agent `smabar/<version>`. If an entry's description contains images, smabar loads them from GitHub as well. GitHub, Inc., 88 Colin P. Kelly Jr. Street, San Francisco, CA 94107, USA, receives your IP address; GitHub's privacy statement applies.
- **Python runtime.** Plugins written in Python need an interpreter; the bundled plugins are such plugins. The first time one starts, the uv tool shipped with smabar downloads a CPython 3.14 build once from the python-build-standalone releases on GitHub (github.com, objects.githubusercontent.com) and stores it under `~/.smabar/tools/`. GitHub receives your IP address as described above. Nothing is installed system-wide.
- **Icons of pinned websites.** When you pin a website to the bar, smabar fetches its icon directly from that website (`/apple-touch-icon.png`, `/favicon.ico`) and stores it under `~/.smabar/cache/icons/`. No third-party icon service is involved.
- **Google Fonts.** Only when you or an agent you connected choose a Google font in the settings or in a theme, smabar downloads the font files once from fonts.googleapis.com and fonts.gstatic.com (Google Ireland Limited, Gordon House, Barrow Street, Dublin 4, Ireland) and stores them under `~/.smabar/cache/fonts/google/`. The bundled themes use system fonts and trigger no download.
- **Community plugins and agents.** Plugins from the Community Store are programs of their own and can open their own connections, for example to a parcel service or a device on your network. Which data such a plugin transfers is decided by the person who published it; this notice does not cover it. smabar's MCP server accepts connections from your own computer only (127.0.0.1). What an agent you connected does with the data it reads is governed by that agent's provider.

## 3. Legal basis, retention and objection

The legal basis for these connections is Art. 6(1)(f) GDPR. Our legitimate interest is providing updates and the signed catalog and supplying the content and plugins you chose with data. On our servers your IP address appears only in technical logs, which are deleted after 14 days at the latest unless a security incident requires longer retention. Cloudflare, GitHub and Google may also process data in the USA; all three are certified under the EU-U.S. Data Privacy Framework. For Open-Meteo and CoinGecko, their own statements on the place of processing apply.

You can object to these connections by not using the respective feature: deactivate the Weather and Crypto plugins, add no world clock, no store installation, no Google font, no pinned website. The update check cannot be switched off separately in direct installations; if you do not want it, use the Microsoft Store version or uninstall smabar.

## 4. Local data

smabar stores settings, plugins, plugin data, caches and logs under `~/.smabar` (Linux and macOS; Windows: `%USERPROFILE%\.smabar`). This data does not leave your computer; logs are not sent to us. You can inspect and delete the data at any time. Removing a plugin deletes its code, data and logs; uninstalling smabar leaves `~/.smabar` in place.

## 5. Your rights

Under the GDPR you have the right of access (Art. 15), rectification (Art. 16), erasure (Art. 17), restriction of processing (Art. 18), data portability (Art. 20) and objection (Art. 21). You may lodge a complaint with a data protection supervisory authority, for example the Landesbeauftragte für Datenschutz und Informationsfreiheit Nordrhein-Westfalen (the data protection authority of North Rhine-Westphalia), Kavalleriestraße 2-4, 40213 Düsseldorf, Germany. Send requests to hello@smabar.com. Because smabar keeps no accounts and IP addresses appear only briefly in logs, we usually cannot link a request to a person (Art. 11 GDPR).

We update this notice when the app or the services used change. The date at the top names the current version; no consent is required for that.
