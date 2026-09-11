import type { ReactNode } from "react";

/**
 * Two short setting groups side by side. The pair is a layout decision, not a
 * grouping one: each child keeps its own title and its own sub-navigation
 * entry, and below 52rem of panel width the columns stack again.
 */
export function SettingColumns({ children }: { children: ReactNode }) {
  return <div className="settings-columns">{children}</div>;
}
