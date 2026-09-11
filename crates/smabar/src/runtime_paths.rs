//! Runtime executable discovery shared by startup and headless provisioning.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// `None` leaves the existing PATH lookup to the core runtime provisioner.
pub fn resolve_uv(package_info: &tauri::PackageInfo) -> Option<PathBuf> {
    let resource_dir = tauri::utils::platform::resource_dir(package_info, &tauri::Env::default())
        .inspect_err(|error| {
            tracing::warn!(%error, "cannot locate bundled runtime resources; set SMABAR_UV or install uv on PATH");
        })
        .ok();
    let executable = std::env::current_exe()
        .inspect_err(|error| {
            tracing::warn!(%error, "cannot locate smabar's adjacent uv; set SMABAR_UV or install uv on PATH");
        })
        .ok();
    select_uv(
        std::env::var_os("SMABAR_UV"),
        resource_dir.as_deref(),
        executable.as_deref(),
    )
}

fn select_uv(
    override_path: Option<OsString>,
    resource_dir: Option<&Path>,
    executable: Option<&Path>,
) -> Option<PathBuf> {
    override_path
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let name = format!("uv{}", std::env::consts::EXE_SUFFIX);
            let private = resource_dir.map(|dir| dir.join("tools").join(&name));
            let adjacent = executable.and_then(Path::parent).map(|dir| dir.join(name));
            [private, adjacent]
                .into_iter()
                .flatten()
                .find(|path| path.is_file())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_prefers_override_then_private_then_adjacent_then_path() {
        let root = tempfile::tempdir().unwrap();
        let resources = root.path().join("lib/smabar");
        let executable = root.path().join("bin/smabar");
        let name = format!("uv{}", std::env::consts::EXE_SUFFIX);
        let private = resources.join("tools").join(&name);
        let adjacent = executable.parent().unwrap().join(name);
        std::fs::create_dir_all(private.parent().unwrap()).unwrap();
        std::fs::create_dir_all(adjacent.parent().unwrap()).unwrap();
        std::fs::write(&private, b"private uv").unwrap();
        std::fs::write(&adjacent, b"adjacent uv").unwrap();

        let resolve = |override_path| select_uv(override_path, Some(&resources), Some(&executable));
        let explicit = root.path().join("explicit-uv");
        // An explicit override remains authoritative even when missing: the
        // provisioner reports its failure instead of silently choosing another uv.
        assert_eq!(
            resolve(Some(explicit.clone().into_os_string())),
            Some(explicit)
        );
        assert_eq!(resolve(None), Some(private.clone()));
        assert_eq!(resolve(Some(OsString::new())), Some(private.clone()));
        std::fs::remove_file(&private).unwrap();
        assert_eq!(resolve(None), Some(adjacent.clone()));
        std::fs::remove_file(adjacent).unwrap();
        assert_eq!(resolve(None), None);
        assert_eq!(select_uv(None, None, None), None);
    }
}
