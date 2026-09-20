import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState } from "react";
import { reportError } from "../../ipc/log";

/** Cargo.toml owns the version; browser previews have no native build. */
export function useAppVersion(): string {
  const [version, setVersion] = useState("dev");
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let cancelled = false;
    getVersion()
      .then((current) => {
        if (!cancelled) setVersion(current);
      })
      .catch(reportError);
    return () => {
      cancelled = true;
    };
  }, []);
  return version;
}
