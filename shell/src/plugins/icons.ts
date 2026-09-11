/**
 * Curated lucide icon registry + trusted SVG serializer.
 *
 * Plugins reference icons as `<span data-lucide="name">`; the sanitizer
 * keeps svg markup out, so the shell builds the SVG itself from the
 * imported lucide icon nodes AFTER sanitizing (see ShadowHost). Unknown
 * names fall back to a neutral circle. The name list is mirrored in
 * ui-kit/contract.json; an anti-drift test keeps them equal.
 */
import type { IconNode } from "lucide-react";

import { __iconNode as activity } from "lucide-react/dist/esm/icons/activity.mjs";
import { __iconNode as alarmClock } from "lucide-react/dist/esm/icons/alarm-clock.mjs";
import { __iconNode as archive } from "lucide-react/dist/esm/icons/archive.mjs";
import { __iconNode as arrowDown } from "lucide-react/dist/esm/icons/arrow-down.mjs";
import { __iconNode as arrowLeft } from "lucide-react/dist/esm/icons/arrow-left.mjs";
import { __iconNode as arrowRight } from "lucide-react/dist/esm/icons/arrow-right.mjs";
import { __iconNode as arrowUp } from "lucide-react/dist/esm/icons/arrow-up.mjs";
import { __iconNode as battery } from "lucide-react/dist/esm/icons/battery.mjs";
import { __iconNode as batteryCharging } from "lucide-react/dist/esm/icons/battery-charging.mjs";
import { __iconNode as bell } from "lucide-react/dist/esm/icons/bell.mjs";
import { __iconNode as bellOff } from "lucide-react/dist/esm/icons/bell-off.mjs";
import { __iconNode as bitcoin } from "lucide-react/dist/esm/icons/bitcoin.mjs";
import { __iconNode as bookmark } from "lucide-react/dist/esm/icons/bookmark.mjs";
import { __iconNode as box } from "lucide-react/dist/esm/icons/box.mjs";
import { __iconNode as bug } from "lucide-react/dist/esm/icons/bug.mjs";
import { __iconNode as calendar } from "lucide-react/dist/esm/icons/calendar.mjs";
import { __iconNode as calendarDays } from "lucide-react/dist/esm/icons/calendar-days.mjs";
import { __iconNode as chartColumn } from "lucide-react/dist/esm/icons/chart-column.mjs";
import { __iconNode as chartLine } from "lucide-react/dist/esm/icons/chart-line.mjs";
import { __iconNode as chartPie } from "lucide-react/dist/esm/icons/chart-pie.mjs";
import { __iconNode as check } from "lucide-react/dist/esm/icons/check.mjs";
import { __iconNode as chevronDown } from "lucide-react/dist/esm/icons/chevron-down.mjs";
import { __iconNode as chevronLeft } from "lucide-react/dist/esm/icons/chevron-left.mjs";
import { __iconNode as chevronRight } from "lucide-react/dist/esm/icons/chevron-right.mjs";
import { __iconNode as chevronUp } from "lucide-react/dist/esm/icons/chevron-up.mjs";
import { __iconNode as circle } from "lucide-react/dist/esm/icons/circle.mjs";
import { __iconNode as circleAlert } from "lucide-react/dist/esm/icons/circle-alert.mjs";
import { __iconNode as circleCheck } from "lucide-react/dist/esm/icons/circle-check.mjs";
import { __iconNode as circleQuestionMark } from "lucide-react/dist/esm/icons/circle-question-mark.mjs";
import { __iconNode as circleX } from "lucide-react/dist/esm/icons/circle-x.mjs";
import { __iconNode as clock } from "lucide-react/dist/esm/icons/clock.mjs";
import { __iconNode as cloud } from "lucide-react/dist/esm/icons/cloud.mjs";
import { __iconNode as cloudDrizzle } from "lucide-react/dist/esm/icons/cloud-drizzle.mjs";
import { __iconNode as cloudFog } from "lucide-react/dist/esm/icons/cloud-fog.mjs";
import { __iconNode as cloudLightning } from "lucide-react/dist/esm/icons/cloud-lightning.mjs";
import { __iconNode as cloudMoon } from "lucide-react/dist/esm/icons/cloud-moon.mjs";
import { __iconNode as cloudRain } from "lucide-react/dist/esm/icons/cloud-rain.mjs";
import { __iconNode as cloudSnow } from "lucide-react/dist/esm/icons/cloud-snow.mjs";
import { __iconNode as cloudSun } from "lucide-react/dist/esm/icons/cloud-sun.mjs";
import { __iconNode as copy } from "lucide-react/dist/esm/icons/copy.mjs";
import { __iconNode as cpu } from "lucide-react/dist/esm/icons/cpu.mjs";
import { __iconNode as creditCard } from "lucide-react/dist/esm/icons/credit-card.mjs";
import { __iconNode as database } from "lucide-react/dist/esm/icons/database.mjs";
import { __iconNode as dollarSign } from "lucide-react/dist/esm/icons/dollar-sign.mjs";
import { __iconNode as download } from "lucide-react/dist/esm/icons/download.mjs";
import { __iconNode as droplets } from "lucide-react/dist/esm/icons/droplets.mjs";
import { __iconNode as ellipsis } from "lucide-react/dist/esm/icons/ellipsis.mjs";
import { __iconNode as externalLink } from "lucide-react/dist/esm/icons/external-link.mjs";
import { __iconNode as eye } from "lucide-react/dist/esm/icons/eye.mjs";
import { __iconNode as eyeOff } from "lucide-react/dist/esm/icons/eye-off.mjs";
import { __iconNode as file } from "lucide-react/dist/esm/icons/file.mjs";
import { __iconNode as fileText } from "lucide-react/dist/esm/icons/file-text.mjs";
import { __iconNode as funnel } from "lucide-react/dist/esm/icons/funnel.mjs";
import { __iconNode as flame } from "lucide-react/dist/esm/icons/flame.mjs";
import { __iconNode as folder } from "lucide-react/dist/esm/icons/folder.mjs";
import { __iconNode as folderOpen } from "lucide-react/dist/esm/icons/folder-open.mjs";
import { __iconNode as gauge } from "lucide-react/dist/esm/icons/gauge.mjs";
import { __iconNode as globe } from "lucide-react/dist/esm/icons/globe.mjs";
import { __iconNode as hardDrive } from "lucide-react/dist/esm/icons/hard-drive.mjs";
import { __iconNode as heart } from "lucide-react/dist/esm/icons/heart.mjs";
import { __iconNode as house } from "lucide-react/dist/esm/icons/house.mjs";
import { __iconNode as image } from "lucide-react/dist/esm/icons/image.mjs";
import { __iconNode as info } from "lucide-react/dist/esm/icons/info.mjs";
import { __iconNode as layers } from "lucide-react/dist/esm/icons/layers.mjs";
import { __iconNode as layoutGrid } from "lucide-react/dist/esm/icons/layout-grid.mjs";
import { __iconNode as link } from "lucide-react/dist/esm/icons/link.mjs";
import { __iconNode as list } from "lucide-react/dist/esm/icons/list.mjs";
import { __iconNode as loaderCircle } from "lucide-react/dist/esm/icons/loader-circle.mjs";
import { __iconNode as lock } from "lucide-react/dist/esm/icons/lock.mjs";
import { __iconNode as mail } from "lucide-react/dist/esm/icons/mail.mjs";
import { __iconNode as mapPin } from "lucide-react/dist/esm/icons/map-pin.mjs";
import { __iconNode as memoryStick } from "lucide-react/dist/esm/icons/memory-stick.mjs";
import { __iconNode as menu } from "lucide-react/dist/esm/icons/menu.mjs";
import { __iconNode as messageCircle } from "lucide-react/dist/esm/icons/message-circle.mjs";
import { __iconNode as messageSquare } from "lucide-react/dist/esm/icons/message-square.mjs";
import { __iconNode as minus } from "lucide-react/dist/esm/icons/minus.mjs";
import { __iconNode as monitor } from "lucide-react/dist/esm/icons/monitor.mjs";
import { __iconNode as moon } from "lucide-react/dist/esm/icons/moon.mjs";
import { __iconNode as music } from "lucide-react/dist/esm/icons/music.mjs";
import { __iconNode as network } from "lucide-react/dist/esm/icons/network.mjs";
import { __iconNode as packageIcon } from "lucide-react/dist/esm/icons/package.mjs";
import { __iconNode as pause } from "lucide-react/dist/esm/icons/pause.mjs";
import { __iconNode as pencil } from "lucide-react/dist/esm/icons/pencil.mjs";
import { __iconNode as pin } from "lucide-react/dist/esm/icons/pin.mjs";
import { __iconNode as play } from "lucide-react/dist/esm/icons/play.mjs";
import { __iconNode as plug } from "lucide-react/dist/esm/icons/plug.mjs";
import { __iconNode as plus } from "lucide-react/dist/esm/icons/plus.mjs";
import { __iconNode as power } from "lucide-react/dist/esm/icons/power.mjs";
import { __iconNode as refreshCw } from "lucide-react/dist/esm/icons/refresh-cw.mjs";
import { __iconNode as repeat } from "lucide-react/dist/esm/icons/repeat.mjs";
import { __iconNode as rotateCw } from "lucide-react/dist/esm/icons/rotate-cw.mjs";
import { __iconNode as save } from "lucide-react/dist/esm/icons/save.mjs";
import { __iconNode as search } from "lucide-react/dist/esm/icons/search.mjs";
import { __iconNode as send } from "lucide-react/dist/esm/icons/send.mjs";
import { __iconNode as settings } from "lucide-react/dist/esm/icons/settings.mjs";
import { __iconNode as settings2 } from "lucide-react/dist/esm/icons/settings-2.mjs";
import { __iconNode as share2 } from "lucide-react/dist/esm/icons/share-2.mjs";
import { __iconNode as shield } from "lucide-react/dist/esm/icons/shield.mjs";
import { __iconNode as shieldCheck } from "lucide-react/dist/esm/icons/shield-check.mjs";
import { __iconNode as skipBack } from "lucide-react/dist/esm/icons/skip-back.mjs";
import { __iconNode as skipForward } from "lucide-react/dist/esm/icons/skip-forward.mjs";
import { __iconNode as slidersHorizontal } from "lucide-react/dist/esm/icons/sliders-horizontal.mjs";
import { __iconNode as snowflake } from "lucide-react/dist/esm/icons/snowflake.mjs";
import { __iconNode as sparkles } from "lucide-react/dist/esm/icons/sparkles.mjs";
import { __iconNode as square } from "lucide-react/dist/esm/icons/square.mjs";
import { __iconNode as star } from "lucide-react/dist/esm/icons/star.mjs";
import { __iconNode as sun } from "lucide-react/dist/esm/icons/sun.mjs";
import { __iconNode as sunrise } from "lucide-react/dist/esm/icons/sunrise.mjs";
import { __iconNode as sunset } from "lucide-react/dist/esm/icons/sunset.mjs";
import { __iconNode as tag } from "lucide-react/dist/esm/icons/tag.mjs";
import { __iconNode as terminal } from "lucide-react/dist/esm/icons/terminal.mjs";
import { __iconNode as thermometer } from "lucide-react/dist/esm/icons/thermometer.mjs";
import { __iconNode as timer } from "lucide-react/dist/esm/icons/timer.mjs";
import { __iconNode as trash2 } from "lucide-react/dist/esm/icons/trash-2.mjs";
import { __iconNode as trendingDown } from "lucide-react/dist/esm/icons/trending-down.mjs";
import { __iconNode as trendingUp } from "lucide-react/dist/esm/icons/trending-up.mjs";
import { __iconNode as triangleAlert } from "lucide-react/dist/esm/icons/triangle-alert.mjs";
import { __iconNode as truck } from "lucide-react/dist/esm/icons/truck.mjs";
import { __iconNode as umbrella } from "lucide-react/dist/esm/icons/umbrella.mjs";
import { __iconNode as upload } from "lucide-react/dist/esm/icons/upload.mjs";
import { __iconNode as user } from "lucide-react/dist/esm/icons/user.mjs";
import { __iconNode as users } from "lucide-react/dist/esm/icons/users.mjs";
import { __iconNode as volume2 } from "lucide-react/dist/esm/icons/volume-2.mjs";
import { __iconNode as volumeX } from "lucide-react/dist/esm/icons/volume-x.mjs";
import { __iconNode as wallet } from "lucide-react/dist/esm/icons/wallet.mjs";
import { __iconNode as wifi } from "lucide-react/dist/esm/icons/wifi.mjs";
import { __iconNode as wifiOff } from "lucide-react/dist/esm/icons/wifi-off.mjs";
import { __iconNode as wind } from "lucide-react/dist/esm/icons/wind.mjs";
import { __iconNode as wrench } from "lucide-react/dist/esm/icons/wrench.mjs";
import { __iconNode as x } from "lucide-react/dist/esm/icons/x.mjs";
import { __iconNode as zap } from "lucide-react/dist/esm/icons/zap.mjs";

/** Every icon name plugins may reference via data-lucide. */
export const ICONS: Readonly<Record<string, IconNode>> = {
  activity: activity,
  "alarm-clock": alarmClock,
  archive: archive,
  "arrow-down": arrowDown,
  "arrow-left": arrowLeft,
  "arrow-right": arrowRight,
  "arrow-up": arrowUp,
  battery: battery,
  "battery-charging": batteryCharging,
  bell: bell,
  "bell-off": bellOff,
  bitcoin: bitcoin,
  bookmark: bookmark,
  box: box,
  bug: bug,
  calendar: calendar,
  "calendar-days": calendarDays,
  "chart-column": chartColumn,
  "chart-line": chartLine,
  "chart-pie": chartPie,
  check: check,
  "chevron-down": chevronDown,
  "chevron-left": chevronLeft,
  "chevron-right": chevronRight,
  "chevron-up": chevronUp,
  circle: circle,
  "circle-alert": circleAlert,
  "circle-check": circleCheck,
  "circle-question-mark": circleQuestionMark,
  "circle-x": circleX,
  clock: clock,
  cloud: cloud,
  "cloud-drizzle": cloudDrizzle,
  "cloud-fog": cloudFog,
  "cloud-lightning": cloudLightning,
  "cloud-moon": cloudMoon,
  "cloud-rain": cloudRain,
  "cloud-snow": cloudSnow,
  "cloud-sun": cloudSun,
  copy: copy,
  cpu: cpu,
  "credit-card": creditCard,
  database: database,
  "dollar-sign": dollarSign,
  download: download,
  droplets: droplets,
  ellipsis: ellipsis,
  "external-link": externalLink,
  eye: eye,
  "eye-off": eyeOff,
  file: file,
  "file-text": fileText,
  funnel: funnel,
  flame: flame,
  folder: folder,
  "folder-open": folderOpen,
  gauge: gauge,
  globe: globe,
  "hard-drive": hardDrive,
  heart: heart,
  house: house,
  image: image,
  info: info,
  layers: layers,
  "layout-grid": layoutGrid,
  link: link,
  list: list,
  "loader-circle": loaderCircle,
  lock: lock,
  mail: mail,
  "map-pin": mapPin,
  "memory-stick": memoryStick,
  menu: menu,
  "message-circle": messageCircle,
  "message-square": messageSquare,
  minus: minus,
  monitor: monitor,
  moon: moon,
  music: music,
  network: network,
  package: packageIcon,
  pause: pause,
  pencil: pencil,
  pin: pin,
  play: play,
  plug: plug,
  plus: plus,
  power: power,
  "refresh-cw": refreshCw,
  repeat: repeat,
  "rotate-cw": rotateCw,
  save: save,
  search: search,
  send: send,
  settings: settings,
  "settings-2": settings2,
  "share-2": share2,
  shield: shield,
  "shield-check": shieldCheck,
  "skip-back": skipBack,
  "skip-forward": skipForward,
  "sliders-horizontal": slidersHorizontal,
  snowflake: snowflake,
  sparkles: sparkles,
  square: square,
  star: star,
  sun: sun,
  sunrise: sunrise,
  sunset: sunset,
  tag: tag,
  terminal: terminal,
  thermometer: thermometer,
  timer: timer,
  "trash-2": trash2,
  "trending-down": trendingDown,
  "trending-up": trendingUp,
  "triangle-alert": triangleAlert,
  truck: truck,
  umbrella: umbrella,
  upload: upload,
  user: user,
  users: users,
  "volume-2": volume2,
  "volume-x": volumeX,
  wallet: wallet,
  wifi: wifi,
  "wifi-off": wifiOff,
  wind: wind,
  wrench: wrench,
  x: x,
  zap: zap,
};

const SVG_NS = "http://www.w3.org/2000/svg";

/**
 * Builds a trusted lucide SVG element (24x24 viewBox, stroked with
 * currentColor, sized 1em so it follows the surrounding font size).
 */
export function renderIcon(name: string, doc: Document): SVGSVGElement {
  const node = ICONS[name] ?? circle;
  const svg = doc.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", "1em");
  svg.setAttribute("height", "1em");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  for (const [tag, attrs] of node) {
    const shape = doc.createElementNS(SVG_NS, tag);
    for (const [attr, value] of Object.entries(attrs)) {
      if (attr === "key") continue; // React bookkeeping, not SVG
      shape.setAttribute(attr, value);
    }
    svg.appendChild(shape);
  }
  return svg;
}

/**
 * Replaces the content of every `[data-lucide]` element under `root` with
 * its rendered icon. Runs POST-sanitize on shell-owned DOM.
 */
export function enhanceIcons(root: ParentNode): void {
  for (const el of root.querySelectorAll("[data-lucide]")) {
    const name = el.getAttribute("data-lucide") ?? "";
    el.replaceChildren(renderIcon(name, el.ownerDocument));
  }
}
