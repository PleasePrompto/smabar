import type { CSSProperties } from "react";

import type { ZoneKind } from "../../store/bar";
import { ShortcutZone } from "./ShortcutZone";
import { PluginZone } from "./PluginZone";

export function ZoneContent({
  kind,
  style,
}: {
  kind: ZoneKind;
  style?: CSSProperties;
}) {
  return kind === "shortcuts" ? (
    <ShortcutZone style={style} />
  ) : (
    <PluginZone style={style} />
  );
}
