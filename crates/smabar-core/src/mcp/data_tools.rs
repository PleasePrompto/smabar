//! Reading a plugin's data directory over MCP.
//!
//! Separate from `plugin_tools.rs` for the 500-line limit, and separate in
//! meaning too: that file serves the plugin's CODE, this one serves what the
//! plugin produced at runtime — the cache it renders from, the files it
//! downloaded, the database it keeps. Read-only on purpose: the plugin owns
//! this directory, and an agent editing its state behind its back is how a
//! plugin ends up disagreeing with its own cache.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData as McpError, tool, tool_router};

use super::SmabarMcp;
use super::plugin_tools::{MAX_INLINE_FILE_BYTES, sanitize_rel_path, validate_plugin_id};
use super::plugin_types::{DataFileOut, PluginDataParams, PluginDataResult, PluginFileOut};

/// A listing stops here; a data directory can hold thousands of entries.
const MAX_LISTED_FILES: usize = 500;

#[tool_router(router = data_tool_router, vis = "pub(crate)")]
impl SmabarMcp {
    #[tool(
        description = "Read a plugin's DATA directory (~/.smabar/data/<id>/) — what the \
                       plugin wrote at runtime: caches, downloads, SQLite databases, state. \
                       Without `path` you get a listing with sizes and modification times; \
                       with `path` (relative to that directory) you get one file's UTF-8 \
                       content. Files over 64 KiB and binary files, a SQLite database \
                       included, come back as metadata only — query those from inside the \
                       plugin and log the result with app.log, then read it via plugin_logs. \
                       Use plugin_read for the plugin's SOURCE code instead. Read-only: the \
                       plugin owns these files."
    )]
    pub(super) async fn plugin_data(
        &self,
        Parameters(PluginDataParams { id, path }): Parameters<PluginDataParams>,
    ) -> Result<Json<PluginDataResult>, McpError> {
        validate_plugin_id(&id)?;
        let dir = self.paths.plugin_data_dir(&id);
        let mut result = PluginDataResult {
            plugin_id: id.clone(),
            dir: dir.display().to_string(),
            exists: dir.is_dir(),
            total_bytes: 0,
            truncated_listing: false,
            files: Vec::new(),
            file: None,
        };
        if !result.exists {
            return Ok(Json(result));
        }

        let Some(relative) = path.filter(|value| !value.trim().is_empty()) else {
            let mut files = Vec::new();
            collect_entries(&dir, &dir, &mut files).map_err(|error| {
                McpError::internal_error(format!("cannot list {}: {error}", dir.display()), None)
            })?;
            files.sort_by(|a, b| a.path.cmp(&b.path));
            result.total_bytes = files.iter().map(|file| file.size).sum();
            result.truncated_listing = files.len() > MAX_LISTED_FILES;
            files.truncate(MAX_LISTED_FILES);
            result.files = files;
            return Ok(Json(result));
        };

        let target = dir.join(sanitize_rel_path(&relative)?);
        let metadata = fs::metadata(&target).map_err(|error| {
            McpError::invalid_params(format!("cannot read {}: {error}", target.display()), None)
        })?;
        if metadata.is_dir() {
            return Err(McpError::invalid_params(
                format!("\"{relative}\" is a directory; call without `path` to list it"),
                None,
            ));
        }
        let size = metadata.len();
        let (content, truncated, binary) = if size > MAX_INLINE_FILE_BYTES {
            (None, true, false)
        } else {
            let bytes = fs::read(&target).map_err(|error| {
                McpError::internal_error(format!("cannot read {}: {error}", target.display()), None)
            })?;
            match String::from_utf8(bytes) {
                Ok(text) => (Some(text), false, false),
                Err(_) => (None, false, true),
            }
        };
        result.total_bytes = size;
        result.file = Some(PluginFileOut {
            path: relative,
            size,
            content,
            truncated,
            binary,
        });
        Ok(Json(result))
    }
}

/// Lists files without reading them — a data directory can hold far more
/// bytes than a reply should ever carry.
fn collect_entries(root: &Path, dir: &Path, files: &mut Vec<DataFileOut>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_entries(root, &path, files)?;
            continue;
        }
        files.push(DataFileOut {
            path: path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string(),
            size: metadata.len(),
            modified: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|since| since.as_secs()),
        });
    }
    Ok(())
}
