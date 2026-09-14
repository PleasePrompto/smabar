import { Blocks, ImageOff, Palette, UserRound } from "lucide-react";
import { useState, type ReactNode } from "react";

import { t } from "../../i18n/t";
import { uiLog } from "../../ipc/log";
import type { StoreEntry } from "../../ipc/store";

/** Fixed-size frames survive missing images without moving the surrounding UI. */
function StoreImage({
  src,
  alt,
  className,
  fallback,
}: {
  src: string | null;
  alt: string;
  className: string;
  fallback: ReactNode;
}) {
  const [failed, setFailed] = useState<string | null>(null);
  return (
    <span className={className}>
      {src !== null && src !== failed ? (
        <img
          src={src}
          alt={alt}
          loading="lazy"
          decoding="async"
          referrerPolicy="no-referrer"
          onError={() => {
            setFailed(src);
            uiLog(
              "warn",
              "Store image could not be loaded; check the connection and reopen the page.",
              {
                fields: { component: "store", image: alt, kind: className },
              },
            );
          }}
        />
      ) : (
        fallback
      )}
    </span>
  );
}

export function StoreIcon({ entry }: { entry: StoreEntry }) {
  return (
    <StoreImage
      src={entry.icon}
      alt=""
      className="settings-store-icon"
      fallback={
        entry.kind === "plugin" ? (
          <Blocks aria-hidden="true" />
        ) : (
          <Palette aria-hidden="true" />
        )
      }
    />
  );
}

export function StoreAvatar({ login }: { login: string }) {
  return (
    <StoreImage
      src={`https://github.com/${encodeURIComponent(login)}.png?size=64`}
      alt=""
      className="settings-store-avatar"
      fallback={<UserRound aria-hidden="true" />}
    />
  );
}

function MissingScreenshot() {
  return (
    <span className="settings-store-image-error">
      <ImageOff aria-hidden="true" />
      {t("settings.store.imageUnavailable")}
    </span>
  );
}

export function StoreGallery({ entry }: { entry: StoreEntry }) {
  if (entry.screenshots.length === 0) return null;
  return (
    <section
      className="settings-store-gallery"
      aria-label={t("settings.store.gallery")}
    >
      <div className="sb-section">{t("settings.store.gallery")}</div>
      <ul
        className="settings-store-gallery-strip"
        tabIndex={0}
        aria-label={t("settings.store.gallery")}
      >
        {entry.screenshots.map((src, index) => {
          const alt = t("settings.store.galleryAlt")
            .replace("{name}", entry.name)
            .replace("{n}", String(index + 1));
          return (
            <li key={`${src}-${String(index)}`}>
              <StoreImage
                src={src}
                alt={alt}
                className="settings-store-screenshot"
                fallback={<MissingScreenshot />}
              />
            </li>
          );
        })}
      </ul>
    </section>
  );
}
