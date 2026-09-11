use std::io;
use std::path::Path;

pub const NAME: &str = "libsmabar-provider-identity.so";

/// Tauri copies resources in place. Unlink this library first so a running
/// WebKit keeps its old inode, including the loader's relocated memory pages.
pub fn detach_loaded_extension(out_dir: &Path) -> io::Result<()> {
    // Match tauri-build: target/[<triple>/]<profile>/build/<package>/out.
    let target_dir = out_dir.ancestors().nth(3).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "unexpected Cargo OUT_DIR layout",
        )
    })?;
    let extension = target_dir.join("web-extensions").join(NAME);
    match std::fs::remove_file(extension) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
