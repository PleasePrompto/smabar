//! Offline website export. No supervisor, user profile or MCP connection is needed.
use std::error::Error;
use std::io;
use std::path::Path;

use serde_json::json;
use smabar_core::{plugins::PluginManifest, themes};

fn main() -> Result<(), Box<dyn Error>> {
    // Optional directories let the documentation check validate extracted examples
    // through the same parser as the installed app, without running plugin code.
    for directory in std::env::args().skip(1) {
        PluginManifest::load(Path::new(&directory))?;
    }
    serde_json::to_writer_pretty(
        io::stdout().lock(),
        &json!({
            "version": env!("CARGO_PKG_VERSION"),
            "manifestSchema": schemars::schema_for!(PluginManifest),
            "theme": themes::contract::export(),
        }),
    )?;
    Ok(())
}
