import { useEffect } from "react";

import { useSmabar } from "../store/bar";
import { refreshCommunityBadge } from "../ipc/store";

/**
 * Browser-dev fixture: seeds sample shortcuts so the dock zone has content
 * without the Tauri core. Everything else (layout switching, toggles, pins)
 * runs through the real settings panel, whose commands ipc/call.ts serves
 * from ipc/fixture.ts in the browser. DEV is statically false in prod
 * builds, so the body is dead-code-eliminated there; in the Tauri window
 * the core owns the state and the fixture stays away.
 */
export function DevFixture() {
  if (!import.meta.env.DEV || "__TAURI_INTERNALS__" in window) return null;
  return <DevFixtureBody />;
}

function DevFixtureBody() {
  useEffect(() => {
    useSmabar.setState({ updateChannel: "app" });
    void import("../ipc/fixture").then(({ seedShortcuts }) => {
      seedShortcuts();
    });
    void import("../ipc/fixturePluginTiles").then(({ seedPluginDemos }) => {
      seedPluginDemos();
    });
    void import("../ipc/fixtureStore").then(({ seedFixtureStoreInstalls }) => {
      seedFixtureStoreInstalls();
      void refreshCommunityBadge();
    });
  }, []);
  return null;
}
