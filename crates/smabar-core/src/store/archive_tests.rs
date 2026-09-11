use std::fs;

use super::archive::{ArchiveError, ArchiveLimits, archive_root, extract_plugin};
use super::testing::{ZipEntry, write_zip};

const COMMIT: &str = "79ae34ab087c5f242622bbdb25510e6b328fb0d3";

fn root() -> String {
    archive_root("smabar-plugins", COMMIT)
}

fn file(name: &str, bytes: &[u8]) -> ZipEntry {
    ZipEntry::File {
        name: format!("{}{name}", root()),
        bytes: bytes.to_vec(),
        mode: None,
        deflate: false,
    }
}

#[test]
fn extracts_only_the_listed_folder_and_reproduces_its_tree_oid() {
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = write_zip(
        dir.path(),
        &[
            ZipEntry::Dir(root()),
            file("LICENSE", b"MIT"),
            file("README.md", b"# repo"),
            file(
                "plugins/hello/smabar.json",
                include_bytes!("fixtures/hello/smabar.json"),
            ),
            ZipEntry::File {
                name: format!("{}plugins/hello/plugin.py", root()),
                bytes: include_bytes!("fixtures/hello/plugin.py").to_vec(),
                mode: None,
                deflate: true,
            },
            file(
                "plugins/hello/README.md",
                include_bytes!("fixtures/hello/README.md"),
            ),
            file("plugins/counter/smabar.json", b"{}"),
        ],
    );
    let dest = dir.path().join("out");
    let extracted = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        "plugins/hello",
        &dest,
        &ArchiveLimits::DEFAULT,
    )
    .expect("extracts");
    assert_eq!(
        extracted.tree_oid,
        "13bd1621878647c0496f62e5559e72898afe1b0e"
    );
    assert_eq!(extracted.files, 3);
    assert!(dest.join("plugin.py").is_file());
    assert!(!dest.join("LICENSE").exists());
    assert!(!dest.join("plugins").exists());
    assert_eq!(
        extracted.manifest,
        include_bytes!("fixtures/hello/smabar.json")
    );
}

#[test]
fn a_root_plugin_uses_the_archive_root_itself() {
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = write_zip(
        dir.path(),
        &[file("smabar.json", b"{}"), file("plugin.py", b"print(1)\n")],
    );
    let dest = dir.path().join("out");
    let extracted = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dest,
        &ArchiveLimits::DEFAULT,
    )
    .expect("extracts");
    assert_eq!(extracted.files, 2);
    assert!(dest.join("smabar.json").is_file());
}

#[test]
fn the_executable_bit_comes_from_the_archive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = write_zip(
        dir.path(),
        &[
            file("smabar.json", b"{}"),
            ZipEntry::File {
                name: format!("{}run.sh", root()),
                bytes: b"#!/bin/sh\n".to_vec(),
                mode: Some(0o100755),
                deflate: false,
            },
        ],
    );
    let dest = dir.path().join("out");
    let executable = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dest,
        &ArchiveLimits::DEFAULT,
    )
    .expect("extracts");
    let zip = write_zip(
        dir.path(),
        &[file("smabar.json", b"{}"), file("run.sh", b"#!/bin/sh\n")],
    );
    let plain = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("out2"),
        &ArchiveLimits::DEFAULT,
    )
    .expect("extracts");
    assert_ne!(executable.tree_oid, plain.tree_oid);
}

#[test]
fn missing_manifest_and_foreign_entries_are_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = write_zip(dir.path(), &[file("plugin.py", b"")]);
    let error = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("a"),
        &ArchiveLimits::DEFAULT,
    )
    .expect_err("no manifest");
    assert!(matches!(error, ArchiveError::NoManifest { .. }), "{error}");

    let zip = write_zip(
        dir.path(),
        &[
            file("smabar.json", b"{}"),
            ZipEntry::File {
                name: "other-repo/x".to_string(),
                bytes: Vec::new(),
                mode: None,
                deflate: false,
            },
        ],
    );
    let error = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("b"),
        &ArchiveLimits::DEFAULT,
    )
    .expect_err("foreign");
    assert!(
        matches!(error, ArchiveError::ForeignEntry { .. }),
        "{error}"
    );
}

#[test]
fn unsafe_names_symlinks_and_duplicates_are_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, expect_unsafe) in [
        ("../escape", true),
        ("sub/../up", true),
        ("nul.txt", true),
        ("COM1", true),
        ("trailing.", true),
        ("back\\slash", true),
        ("fine/name.txt", false),
    ] {
        let zip = write_zip(dir.path(), &[file("smabar.json", b"{}"), file(name, b"x")]);
        let result = extract_plugin(
            &zip,
            "smabar-plugins",
            COMMIT,
            ".",
            &dir.path().join(name.len().to_string()),
            &ArchiveLimits::DEFAULT,
        );
        assert_eq!(
            matches!(result, Err(ArchiveError::UnsafePath(_))),
            expect_unsafe,
            "{name}: {result:?}"
        );
    }
    let zip = write_zip(
        dir.path(),
        &[
            file("smabar.json", b"{}"),
            ZipEntry::Symlink {
                name: format!("{}link", root()),
                target: "smabar.json".to_string(),
            },
        ],
    );
    let error = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("s"),
        &ArchiveLimits::DEFAULT,
    )
    .expect_err("symlink");
    assert!(matches!(error, ArchiveError::Symlink(_)), "{error}");

    let zip = write_zip(
        dir.path(),
        &[
            file("smabar.json", b"{}"),
            file("A.txt", b"1"),
            file("a.txt", b"2"),
        ],
    );
    let error = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("d"),
        &ArchiveLimits::DEFAULT,
    )
    .expect_err("duplicate");
    assert!(matches!(error, ArchiveError::Duplicate(_)), "{error}");
}

#[test]
fn limits_stop_bombs_before_they_land() {
    let dir = tempfile::tempdir().expect("tempdir");
    let limits = ArchiveLimits {
        max_files: 2,
        max_directories: 1,
        max_depth: 2,
        max_file_bytes: 5,
        max_total_bytes: 6,
    };
    let zip = write_zip(
        dir.path(),
        &[file("smabar.json", b"{}"), file("a", b""), file("b", b"")],
    );
    let error = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("1"),
        &limits,
    )
    .expect_err("files");
    assert!(matches!(error, ArchiveError::TooManyFiles(2)), "{error}");

    let zip = write_zip(
        dir.path(),
        &[file("smabar.json", b"{}"), file("big.bin", b"123456")],
    );
    let error = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("2"),
        &limits,
    )
    .expect_err("size");
    assert!(
        matches!(error, ArchiveError::FileTooLarge { .. }),
        "{error}"
    );

    let zip = write_zip(
        dir.path(),
        &[file("smabar.json", b"{}"), file("x", b"12345")],
    );
    let error = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("3"),
        &limits,
    )
    .expect_err("total");
    assert!(matches!(error, ArchiveError::TotalTooLarge(6)), "{error}");

    let zip = write_zip(
        dir.path(),
        &[file("smabar.json", b"{}"), file("a/b/c", b"")],
    );
    let error = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("4"),
        &limits,
    )
    .expect_err("depth");
    assert!(matches!(error, ArchiveError::TooDeep(_)), "{error}");

    let zip = write_zip(dir.path(), &[file("d1/x", b""), file("d2/y", b"")]);
    let error = extract_plugin(
        &zip,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("5"),
        &limits,
    )
    .expect_err("dirs");
    assert!(
        matches!(error, ArchiveError::TooManyDirectories(1)),
        "{error}"
    );
}

#[test]
fn a_corrupt_archive_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("broken.zip");
    fs::write(&path, b"not a zip").expect("write");
    let error = extract_plugin(
        &path,
        "smabar-plugins",
        COMMIT,
        ".",
        &dir.path().join("x"),
        &ArchiveLimits::DEFAULT,
    )
    .expect_err("broken");
    assert!(matches!(error, ArchiveError::NotAZip { .. }), "{error}");
}
