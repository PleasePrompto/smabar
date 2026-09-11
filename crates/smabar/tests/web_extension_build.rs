use std::fs::{self, File};
use std::io::Read;

#[test]
fn rebuilding_preserves_the_extension_already_open_in_webkit() {
    let root = tempfile::tempdir().unwrap();
    let target_dir = root.path().join("debug");
    let out_dir = target_dir.join("build/smabar-test/out");
    let resource = target_dir.join("web-extensions/libsmabar-provider-identity.so");
    build_extension::detach_loaded_extension(&out_dir).unwrap();
    fs::create_dir_all(&out_dir).unwrap();
    fs::create_dir_all(resource.parent().unwrap()).unwrap();
    fs::write(&resource, b"loaded extension").unwrap();
    let mut loaded = File::open(&resource).unwrap();
    let replacement = out_dir.join("extension.so");
    fs::write(&replacement, b"rebuilt extension").unwrap();

    // tauri-build copies resources over their existing target paths.
    build_extension::detach_loaded_extension(&out_dir).unwrap();
    fs::copy(&replacement, &resource).unwrap();

    let mut previous = Vec::new();
    loaded.read_to_end(&mut previous).unwrap();
    assert_eq!(previous, b"loaded extension");
    assert_eq!(fs::read(&resource).unwrap(), b"rebuilt extension");
}
#[path = "../build_extension.rs"]
mod build_extension;
