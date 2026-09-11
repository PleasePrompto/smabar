import {
  Blocks,
  ChevronRight,
  LayoutPanelTop,
  Link2,
  MonitorCog,
  Palette,
  ScrollText,
  X,
} from "lucide-react";
import {
  Fragment,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

import { t } from "../../i18n/t";
import { closeCurrentSurface } from "../../ipc/surface";
import { reportError } from "../../ipc/log";
import { useSmabar } from "../../store/bar";
import { ResizeEdges } from "./ResizeEdges";
import { groupOf, pageOf, pagesOf, type SettingsPage } from "./settingsPages";
import { StorePage } from "./StorePage";
import { scrollToGroup, useSubsections } from "./useSubsections";
import { BarTab } from "./BarTab";
import { DesignTab } from "./DesignTab";
import { LegalTab } from "./LegalTab";
import { ShortcutsTab } from "./ShortcutsTab";
import { SystemTab } from "./SystemTab";
import { PluginsTab } from "./PluginsTab";

/**
 * The one group the panel shows while the terms of use are not accepted.
 * Never listed otherwise: once accepted, the texts sit folded under System,
 * and a request for `legal` lands there.
 */
const LEGAL_GROUP = { id: "legal", Body: LegalTab, Icon: ScrollText } as const;

/**
 * The panel's sections, in order. Each one is named after the THING it
 * configures, so its name says what is inside it — every setting about the
 * shortcut tiles lives under Shortcuts, whether it is a size, a spacing or a
 * hover effect.
 */
const GROUPS = [
  { id: "bar", Body: BarTab, Icon: LayoutPanelTop },
  { id: "design", Body: DesignTab, Icon: Palette },
  { id: "shortcuts", Body: ShortcutsTab, Icon: Link2 },
  { id: "plugins", Body: PluginsTab, Icon: Blocks },
  { id: "system", Body: SystemTab, Icon: MonitorCog },
] as const;

type Group = (typeof GROUPS)[number] | typeof LEGAL_GROUP;

/**
 * The settings panel; renders nothing while closed. The selected group
 * lives in the store, so a context menu can open the panel directly on one
 * ("Plugin settings…"); unknown ids fall back to the bar group.
 */
export function SettingsPanel({ preview = false }: { preview?: boolean }) {
  return <PanelBody preview={preview} />;
}

function PanelBody({ preview }: { preview: boolean }) {
  const group = useSmabar((state) => state.settingsGroup);
  const setGroup = useSmabar((state) => state.setSettingsGroup);
  // Only the browser preview sizes itself from the config; the native
  // window is sized by the window manager and the core remembers it, so
  // the selector yields a stable null there and never re-renders the panel.
  const settingsWindow = useSmabar((state) =>
    preview ? state.settingsWindow : null,
  );
  // Until the terms are accepted the panel is the legal group alone: no
  // other section, no store page, whatever group was asked for.
  const gated = useSmabar((state) => state.legalRequired);
  const shown: readonly Group[] = gated ? [LEGAL_GROUP] : GROUPS;
  // A page id such as `plugins/store` keeps its group highlighted while the
  // body shows the page instead of the group's sections.
  const page = gated ? null : pageOf(group);
  const requested = groupOf(group) === "legal" ? "system" : groupOf(group);
  const current: Group = gated
    ? LEGAL_GROUP
    : (GROUPS.find((entry) => entry.id === requested) ?? GROUPS[0]);
  const closeRef = useRef<HTMLButtonElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const subsections = useSubsections(bodyRef, current.id, page === null);
  // Counted so the page entry can be clicked again from a detail page: the
  // group id does not change then, and remounting the page is what shows
  // the list.
  const [pageOpenings, setPageOpenings] = useState(0);
  // A section entry clicked while a page is open first leaves the page;
  // the scroll follows once the group's sections are rendered again.
  const pendingScroll = useRef<{ group: string; index: number } | null>(null);
  useEffect(() => {
    const pending = pendingScroll.current;
    if (pending === null) return;
    if (pending.group !== current.id) {
      pendingScroll.current = null;
    } else if (page === null && scrollToGroup(bodyRef.current, pending.index)) {
      pendingScroll.current = null;
    }
  }, [page, current.id, subsections]);
  const hasSubnav = (id: string) =>
    subsections.length > 0 || pagesOf(id).length > 0;
  const pageEntry = (entry: SettingsPage) => (
    <li key={entry.id}>
      <button
        type="button"
        className={
          page?.id === entry.id
            ? "settings-subnav-page sb-active"
            : "settings-subnav-page"
        }
        aria-current={page?.id === entry.id ? "page" : undefined}
        onClick={() => {
          pendingScroll.current = null;
          setGroup(entry.id);
          setPageOpenings((count) => count + 1);
        }}
      >
        {t(entry.labelKey)}
        <ChevronRight size="1em" aria-hidden="true" />
      </button>
    </li>
  );
  const native = !preview && "__TAURI_INTERNALS__" in window;

  useEffect(() => {
    closeRef.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      void closeCurrentSurface().catch(reportError);
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const close = () => {
    void closeCurrentSurface().catch(reportError);
  };
  const beginDrag = () => {
    if (!native) return;
    void getCurrentWindow().startDragging().catch(reportError);
  };
  const resizeWithKeyboard = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (preview || !("__TAURI_INTERNALS__" in window)) return;
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
  const { Body } = current;
  const previewStyle: CSSProperties | undefined =
    settingsWindow === null
      ? undefined
      : {
          left: "50%",
          top: "50%",
          width: Math.min(settingsWindow.width, window.innerWidth - 8),
          height: Math.min(settingsWindow.height, window.innerHeight - 8),
          transform: "translate(-50%, -50%)",
        };

  return (
    <div
      className={`sb-root sb-flush settings-panel ${preview ? "fixed" : "absolute inset-0"}`}
      style={previewStyle}
      data-capture="settings"
      role="dialog"
      aria-labelledby="settings-title"
      onClick={(event) => {
        event.stopPropagation();
      }}
      onAuxClick={(event) => {
        event.stopPropagation();
      }}
    >
      <div className="settings-window settings-panel-surface text-[color:var(--sb-text)]">
        <div
          className="sb-header settings-panel-header touch-none select-none cursor-grab active:cursor-grabbing"
          onMouseDown={(event) => {
            const target = event.target;
            if (
              event.button === 0 &&
              target instanceof Element &&
              target.closest("button") === null
            ) {
              beginDrag();
            }
          }}
        >
          <div className="settings-title">
            <span className="settings-brand-mark" aria-hidden="true" />
            <h1 id="settings-title" className="sb-title">
              {t("settings.title")}
            </h1>
          </div>
          <div className="sb-header-actions">
            <button
              ref={closeRef}
              className="sb-btn sb-btn-ghost sb-btn-icon"
              aria-label={t("settings.close")}
              onClick={() => {
                close();
              }}
            >
              <X size="1em" />
            </button>
          </div>
        </div>

        <div className="settings-workspace">
          {/* A plain nav, not a tablist: a tablist may only own tabs, and the
              sub-entries below the open section are not tabs — they scroll
              within the one panel. The tab roles were a half-promise anyway
              (no arrow-key navigation, no roving tabindex), so aria-current
              says more truthfully what these buttons do. */}
          <nav className="settings-group-nav" aria-label={t("settings.groups")}>
            {shown.map(({ id, Icon }) => (
              <div key={id} className="settings-nav-section">
                <button
                  id={`settings-tab-${id}`}
                  type="button"
                  aria-label={t(`settings.group.${id}`)}
                  aria-current={current.id === id ? "page" : undefined}
                  aria-controls="settings-content"
                  aria-expanded={
                    current.id === id && hasSubnav(id) ? true : undefined
                  }
                  className={current.id === id ? "sb-active" : undefined}
                  onClick={() => {
                    pendingScroll.current = null;
                    setGroup(id);
                  }}
                >
                  <Icon
                    className="settings-nav-icon"
                    size="1em"
                    aria-hidden="true"
                  />
                  <span className="settings-nav-label">
                    {t(`settings.group.${id}`)}
                  </span>
                </button>
                {current.id === id && hasSubnav(id) && (
                  <ul
                    className="settings-subnav"
                    aria-label={t("settings.subsections")}
                  >
                    {subsections.map((entry) => (
                      <Fragment key={entry.index}>
                        <li>
                          <button
                            type="button"
                            onClick={() => {
                              if (page === null) {
                                pendingScroll.current = null;
                                scrollToGroup(bodyRef.current, entry.index);
                                return;
                              }
                              setGroup(current.id);
                              pendingScroll.current = {
                                group: current.id,
                                index: entry.index,
                              };
                            }}
                          >
                            {entry.label}
                          </button>
                        </li>
                        {pagesOf(id)
                          .filter(
                            (candidate) => candidate.after === entry.index,
                          )
                          .map(pageEntry)}
                      </Fragment>
                    ))}
                    {pagesOf(id)
                      .filter(
                        (candidate) =>
                          candidate.after === undefined ||
                          candidate.after >= subsections.length,
                      )
                      .map(pageEntry)}
                  </ul>
                )}
              </div>
            ))}
          </nav>
          <div
            key={group}
            ref={bodyRef}
            id="settings-content"
            className="surface-scroll sb-scroll settings-group-content"
            role="region"
            aria-labelledby={`settings-tab-${current.id}`}
            tabIndex={0}
          >
            {page === null ? (
              <Body />
            ) : (
              <StorePage
                key={pageOpenings}
                kind={page.kind}
                onBack={() => {
                  pendingScroll.current = null;
                  setGroup(current.id);
                }}
              />
            )}
          </div>
        </div>
      </div>
      {native && <ResizeEdges />}
      {native && (
        <button
          type="button"
          className="settings-resize-control"
          aria-label={t("settings.resize")}
          title={t("settings.resize")}
          onKeyDown={resizeWithKeyboard}
        />
      )}
    </div>
  );
}
