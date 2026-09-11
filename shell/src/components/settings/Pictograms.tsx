import type { ReactNode } from "react";

import type {
  BarChrome,
  BarPosition,
  BarVariant,
  BarWidth,
  LabelMode,
  LayoutBehavior,
  PopupPosition,
  TileChrome,
  ZOrder,
  ZoneAlign,
  ZoneKind,
} from "../../store/bar";

function MiniSvg({ children }: { children: ReactNode }) {
  return (
    <svg
      className="settings-pictogram"
      viewBox="0 0 32 22"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function PositionPictogram({ value }: { value: BarPosition }) {
  const y = value === "top" ? 3 : 16;
  return (
    <MiniSvg>
      <rect x="2" y="2" width="28" height="18" rx="3" opacity=".45" />
      <rect x="5" y={y} width="22" height="3" rx="1.5" fill="currentColor" />
    </MiniSvg>
  );
}

export function PopupPositionPictogram({ value }: { value: PopupPosition }) {
  const x = value.endsWith("left") ? 4 : value.endsWith("right") ? 21 : 12.5;
  const y = value.startsWith("top") ? 4 : 13;
  return (
    <MiniSvg>
      <rect x="2" y="2" width="28" height="18" rx="3" opacity=".45" />
      <rect x={x} y={y} width="7" height="5" rx="1.5" fill="currentColor" />
    </MiniSvg>
  );
}

export function VariantPictogram({ value }: { value: BarVariant }) {
  return (
    <MiniSvg>
      {value === "split" && (
        <>
          <rect x="2" y="7" width="28" height="9" rx="2" />
          <line x1="13" y1="7" x2="13" y2="16" />
        </>
      )}
      {value === "rows" && (
        <>
          <rect x="2" y="2" width="28" height="7" rx="2" />
          <rect x="2" y="13" width="28" height="7" rx="2" />
        </>
      )}
      {value === "solo" && <rect x="5" y="8" width="22" height="7" rx="2" />}
    </MiniSvg>
  );
}

export function WidthPictogram({ value }: { value: BarWidth }) {
  return (
    <MiniSvg>
      <rect x="2" y="2" width="28" height="18" rx="3" opacity=".35" />
      <rect
        x={value === "full" ? 3 : 8}
        y="14"
        width={value === "full" ? 26 : 16}
        height="4"
        rx="2"
        fill="currentColor"
      />
    </MiniSvg>
  );
}

export function ZonePictogram({ value }: { value: ZoneKind }) {
  return (
    <MiniSvg>
      <rect x="2" y="5" width="28" height="12" rx="3" />
      {value === "shortcuts" ? (
        <>
          <circle cx="9" cy="11" r="2" fill="currentColor" />
          <circle cx="16" cy="11" r="2" fill="currentColor" />
          <circle cx="23" cy="11" r="2" fill="currentColor" />
        </>
      ) : (
        <path d="M7 12h4l2-4 4 7 2-4h6" />
      )}
    </MiniSvg>
  );
}

export function BehaviorPictogram({ value }: { value: LayoutBehavior }) {
  return (
    <MiniSvg>
      <rect x="2" y="2" width="28" height="18" rx="3" opacity=".35" />
      {value === "reserve" && (
        <>
          <rect x="3" y="15" width="26" height="4" rx="1" fill="currentColor" />
          <path d="M6 12v-3m4 3V9m4 3V9" opacity=".6" />
        </>
      )}
      {value === "float" && (
        <rect x="7" y="12" width="18" height="5" rx="2.5" fill="currentColor" />
      )}
      {value === "autohide" && (
        <>
          <line x1="4" y1="18" x2="28" y2="18" strokeWidth="2.5" />
          <path d="m16 14-3-3m3 3 3-3" />
        </>
      )}
    </MiniSvg>
  );
}

export function StackPictogram({ value }: { value: ZOrder }) {
  const frontY = value === "top" ? 5 : 9;
  const backY = value === "top" ? 9 : 5;
  return (
    <MiniSvg>
      <rect x="7" y={backY} width="18" height="9" rx="2" opacity=".35" />
      <rect x="4" y={frontY} width="18" height="9" rx="2" fill="currentColor" />
    </MiniSvg>
  );
}

export function LabelsPictogram({ value }: { value: LabelMode }) {
  return (
    <MiniSvg>
      <rect x="5" y={value === "below" ? 4 : 7} width="8" height="8" rx="2" />
      {value === "right" && <line x1="17" y1="11" x2="27" y2="11" />}
      {value === "below" && <line x1="6" y1="17" x2="18" y2="17" />}
      {value === "hidden" && <path d="M18 8l8 8m0-8-8 8" opacity=".55" />}
    </MiniSvg>
  );
}

export function BarChromePictogram({ value }: { value: BarChrome }) {
  return (
    <MiniSvg>
      <rect
        x="2"
        y="7"
        width="28"
        height="11"
        rx="3"
        fill="currentColor"
        opacity=".2"
        stroke={value === "card" ? "currentColor" : "none"}
      />
      <circle cx="8" cy="12.5" r="1.6" fill="currentColor" />
      <circle cx="14" cy="12.5" r="1.6" fill="currentColor" />
      <line x1="19" y1="12.5" x2="25" y2="12.5" />
    </MiniSvg>
  );
}

export function ChromePictogram({ value }: { value: TileChrome }) {
  return (
    <MiniSvg>
      <rect
        x="5"
        y="4"
        width="22"
        height="14"
        rx="4"
        fill={value === "card" ? "currentColor" : "none"}
        opacity={value === "card" ? ".35" : "1"}
        strokeDasharray={value === "flat" ? "2 2" : undefined}
      />
      <circle cx="11" cy="11" r="2" fill="currentColor" />
      <line x1="16" y1="11" x2="23" y2="11" />
    </MiniSvg>
  );
}

export function AlignPictogram({ value }: { value: ZoneAlign }) {
  const x = value === "left" ? 5 : value === "right" ? 13 : 9;
  return (
    <MiniSvg>
      <rect x="2" y="5" width="28" height="12" rx="3" opacity=".45" />
      <circle cx={x + 2} cy="11" r="2" fill="currentColor" />
      <circle cx={x + 7} cy="11" r="2" fill="currentColor" />
      <circle cx={x + 12} cy="11" r="2" fill="currentColor" />
    </MiniSvg>
  );
}
