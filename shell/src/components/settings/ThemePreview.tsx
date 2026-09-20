import {
  AppWindow,
  Folder,
  Globe,
  Layers,
  Settings,
  Sun,
  Terminal,
} from "lucide-react";
import type { CSSProperties } from "react";
import { t } from "../../i18n/t";
import type { ThemeSummary, ZoneKind } from "../../store/bar";
import {
  BehaviorPictogram,
  PositionPictogram,
  VariantPictogram,
  WidthPictogram,
} from "./Pictograms";

/** A schematic desktop, using the same resolved tokens and activation as the bar. */
export function ThemePreview({
  preview,
  descriptionId,
}: {
  preview: ThemeSummary["preview"];
  descriptionId: string;
}) {
  const { layout, appearance } = preview;
  const full = layout.width === "full";
  const rows = layout.variant === "rows";
  const zones: ZoneKind[] =
    layout.variant === "solo"
      ? [layout.primaryZone]
      : rows &&
          (layout.primaryZone === "plugins") === (layout.position === "top")
        ? ["plugins", "shortcuts"]
        : ["shortcuts", "plugins"];
  // Relative desktop geometry: 1440px is the illustration's reference width.
  const width = full
    ? `${String(Math.min(layout.maxWidth || 1440, 1440) / 14.4)}%`
    : "82%";
  const style: CSSProperties & Record<string, string | number> = {
    ...appearance.tokens,
    "--preview-margin": `${String(layout.margin * 0.3)}px`,
    "--preview-width": width,
    "--preview-split": `${String(layout.dividerRatio * 100)}%`,
  };
  return (
    <>
      <span
        className="settings-theme-desktop"
        data-position={layout.position}
        data-behavior={layout.behavior}
        aria-hidden="true"
      >
        <span className="settings-theme-window">
          <span className="settings-theme-window-toolbar">
            <i />
            <i />
            <i />
          </span>
          <span className="settings-theme-window-content">
            <AppWindow />
            <i />
            <i />
            <i />
          </span>
        </span>
        <span
          className="settings-theme-miniature"
          style={style}
          data-variant={layout.variant}
          data-width={layout.width}
          data-tile-chrome={appearance.tileChrome}
        >
          {rows ? (
            zones.map((zone, index) => (
              <span
                key={zone}
                className="settings-theme-bar surface-bar"
                data-bar-chrome={appearance.barChrome}
              >
                <PreviewZone
                  zone={zone}
                  align={
                    zone === "plugins"
                      ? appearance.pluginAlign
                      : appearance.shortcutAlign
                  }
                />
                {index === 1 && <Settings className="settings-theme-gear" />}
              </span>
            ))
          ) : (
            <span
              className="settings-theme-bar surface-bar"
              data-bar-chrome={appearance.barChrome}
            >
              {layout.variant === "solo" && (
                <Layers className="settings-theme-gear" />
              )}
              {zones.map((zone) => (
                <PreviewZone
                  key={zone}
                  zone={zone}
                  align={
                    zone === "plugins"
                      ? appearance.pluginAlign
                      : appearance.shortcutAlign
                  }
                />
              ))}
              <Settings className="settings-theme-gear" />
            </span>
          )}
        </span>
        {layout.behavior === "autohide" && (
          <span className="settings-theme-edge" />
        )}
      </span>
      <span className="settings-theme-facts" id={descriptionId}>
        <span>
          <PositionPictogram value={layout.position} />
          {t(`settings.pos.${layout.position}`)}
        </span>
        <span>
          <WidthPictogram value={layout.width} />
          {full && layout.maxWidth > 0
            ? `${String(layout.maxWidth)} px`
            : t(`settings.themes.preview.width.${layout.width}`)}
        </span>
        <span>
          <VariantPictogram value={layout.variant} />
          {t(`settings.variant.${layout.variant}`)}
        </span>
        <span title={t(`settings.themes.preview.${layout.behavior}`)}>
          <BehaviorPictogram value={layout.behavior} />
          {t(`settings.behavior.${layout.behavior}`)}
        </span>
      </span>
    </>
  );
}

export function ThemeSwatches({
  preview,
}: {
  preview: ThemeSummary["preview"];
}) {
  const tokens = preview.appearance.tokens;
  return (
    <span className="settings-theme-swatches" style={{ ...tokens }}>
      {[
        { label: "accent", token: "--sb-accent" },
        { label: "accent2", token: "--sb-accent-2" },
        { label: "surface", token: "--sb-bar-bg" },
        { label: "text", token: "--sb-text" },
      ].map(({ label, token }) => {
        const description = `${t(`settings.colors.${label}`)}: ${tokens[token] ?? ""}`;
        return (
          <span
            key={token}
            role="img"
            aria-label={description}
            title={description}
            style={{ background: `var(${token})` }}
          />
        );
      })}
    </span>
  );
}

function PreviewZone({ zone, align }: { zone: ZoneKind; align: string }) {
  return (
    <span className="settings-theme-zone" data-zone={zone} data-align={align}>
      {zone === "shortcuts" ? (
        <>
          <span className="settings-theme-tile">
            <Globe />
          </span>
          <span className="settings-theme-tile">
            <Folder />
          </span>
          <span className="settings-theme-tile">
            <Terminal />
          </span>
        </>
      ) : (
        <>
          <span className="settings-theme-tile settings-theme-clock">
            09:41
          </span>
          <span className="settings-theme-tile">
            <Sun />
            <span>21°</span>
          </span>
        </>
      )}
    </span>
  );
}
