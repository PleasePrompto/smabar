import { t } from "../i18n/t";

/**
 * Atmospheric desktop background — layered gradients + floating blobs + stars.
 * Browser-dev stand-in for a real desktop; never rendered in the Tauri window.
 */
export function DesktopBackground() {
  return (
    <div className="smabar-desktop fixed inset-0 -z-10 overflow-hidden">
      {/* Soft floating blobs */}
      <div
        className="absolute -top-40 -left-40 h-[40rem] w-[40rem] rounded-full opacity-50"
        style={{
          background:
            "radial-gradient(circle, rgba(124, 58, 237, 0.4) 0%, transparent 70%)",
          filter: "blur(80px)",
          animation: "blob 22s ease-in-out infinite",
        }}
      />
      <div
        className="absolute top-1/3 -right-40 h-[36rem] w-[36rem] rounded-full opacity-40"
        style={{
          background:
            "radial-gradient(circle, rgba(14, 165, 233, 0.35) 0%, transparent 70%)",
          filter: "blur(90px)",
          animation: "blob 28s ease-in-out infinite reverse",
        }}
      />
      <div
        className="absolute -bottom-40 left-1/4 h-[38rem] w-[38rem] rounded-full opacity-35"
        style={{
          background:
            "radial-gradient(circle, rgba(236, 72, 153, 0.4) 0%, transparent 70%)",
          filter: "blur(100px)",
          animation: "blob 32s ease-in-out infinite",
        }}
      />

      {/* Subtle noise overlay */}
      <div
        className="absolute inset-0 opacity-[0.04] mix-blend-overlay"
        style={{
          backgroundImage: `url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='200' height='200'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/></filter><rect width='100%' height='100%' filter='url(%23n)'/></svg>")`,
        }}
      />

      {/* Faux desktop "icons" — purely decorative */}
      <DesktopFauxIcons />
    </div>
  );
}

function DesktopFauxIcons() {
  const icons = [
    { labelKey: "desktop.icon.projects", color: "rgba(139, 92, 246, 0.6)" },
    { labelKey: "desktop.icon.screenshots", color: "rgba(14, 165, 233, 0.6)" },
    { labelKey: "desktop.icon.documents", color: "rgba(245, 158, 11, 0.6)" },
  ];
  return (
    <div className="absolute top-6 left-6 flex flex-col gap-3">
      {icons.map((ic) => (
        <div
          key={ic.labelKey}
          className="flex w-20 flex-col items-center gap-1"
        >
          <div
            className="surface-bar flex h-12 w-12 items-center justify-center rounded-xl"
            style={{
              background: `linear-gradient(135deg, ${ic.color}, transparent)`,
            }}
          >
            <div className="h-5 w-5 rounded-sm bg-white/20" />
          </div>
          <span className="text-[10px] text-white/55">{t(ic.labelKey)}</span>
        </div>
      ))}
    </div>
  );
}
