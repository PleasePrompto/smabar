//! Persisted monitor preference. Runtime monitor discovery stays at the app edge.

use serde::{Deserialize, Serialize};

/// One user-selected physical display. `id` is platform-owned and opaque;
/// `label` is retained so a disconnected display remains recognizable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MonitorPreference {
    pub id: String,
    pub label: String,
}
