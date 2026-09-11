use super::treeoid::{TreeBuilder, TreeError};

/// Expected values come from `git write-tree` on a real repository holding
/// exactly these files (`run.sh` added with `--chmod=+x`).
#[test]
fn matches_git_write_tree_for_a_mixed_folder() {
    let mut tree = TreeBuilder::new();
    tree.add_blob("smabar.json", b"{}", false).expect("add");
    tree.add_blob("run.sh", b"#!/bin/sh\n", true).expect("add");
    tree.add_blob("lib/util.py", b"x = 1\n", false)
        .expect("add");
    tree.add_blob(".hidden", b"h\n", false).expect("add");
    // The ordering trap: `a.txt` sorts before the directory `a` ("a/").
    tree.add_blob("a.txt", b"a\n", false).expect("add");
    tree.add_blob("a/b.txt", b"b\n", false).expect("add");
    assert_eq!(tree.finish(), "54da1c46cb80781e309c7c2135a3dde6b173709d");
}

#[test]
fn the_executable_bit_changes_the_oid() {
    let mut plain = TreeBuilder::new();
    plain
        .add_blob("run.sh", b"#!/bin/sh\n", false)
        .expect("add");
    let mut executable = TreeBuilder::new();
    executable
        .add_blob("run.sh", b"#!/bin/sh\n", true)
        .expect("add");
    assert_ne!(plain.finish(), executable.finish());
}

#[test]
fn the_empty_tree_has_gits_well_known_oid() {
    assert_eq!(
        TreeBuilder::new().finish(),
        "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
    );
}

/// The current `hello` fixture, independently hashed with `git write-tree`.
/// The signed catalog fixture keeps its original bytes and signature.
#[test]
fn reproduces_the_git_oid_of_the_hello_plugin() {
    let mut tree = TreeBuilder::new();
    tree.add_blob(
        "smabar.json",
        include_bytes!("fixtures/hello/smabar.json"),
        false,
    )
    .expect("add");
    tree.add_blob(
        "plugin.py",
        include_bytes!("fixtures/hello/plugin.py"),
        false,
    )
    .expect("add");
    tree.add_blob(
        "README.md",
        include_bytes!("fixtures/hello/README.md"),
        false,
    )
    .expect("add");
    assert_eq!(tree.finish(), "13bd1621878647c0496f62e5559e72898afe1b0e");
}

#[test]
fn rejects_paths_that_are_not_plain_relative_files() {
    let mut tree = TreeBuilder::new();
    for bad in ["", "a//b", "../x", "./x", "dir/"] {
        let error = tree.add_blob(bad, b"", false).expect_err(bad);
        assert!(matches!(error, TreeError::InvalidPath(_)));
    }
}
