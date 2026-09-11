//! Restore a previous plugin after the replacement fails to start.

use std::fs;
use std::path::Path;

use super::journal::{self, Journal};
use super::{Inner, StoreError, write_receipt};

/// Puts the previous version back (or removes a fresh install that failed).
/// An interrupted reverse swap uses the same recovery journal as the install.
pub(super) async fn run(
    inner: &Inner,
    transaction: &Journal,
    backup: &Path,
) -> Result<(), StoreError> {
    let id = &transaction.id;
    if transaction.had_previous {
        journal::write(&inner.paths, transaction)?;
        let failed = inner.paths.store_staging_dir().join(format!("{id}.failed"));
        inner.supervisor.replace_dir(id, backup, &failed).await?;
        write_receipt(inner, id, transaction.previous_receipt.clone()).await?;
        journal::clear(&inner.paths)?;
        if let Err(error) = fs::remove_dir_all(&failed) {
            tracing::warn!(%error, path = %failed.display(), "rollback succeeded but failed plugin code remains in staging; it will be cleaned at next startup");
        }
        tracing::warn!(plugin = %id, "the new version failed to start; restored the previous one");
    } else {
        inner
            .supervisor
            .remove(id)
            .await
            .map_err(|source| StoreError::Io {
                action: "remove the failed fresh install",
                path: inner.paths.plugins_dir().join(id),
                source: std::io::Error::other(source),
            })?;
        write_receipt(inner, id, None).await?;
    }
    Ok(())
}
