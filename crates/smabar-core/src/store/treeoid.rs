//! Git tree object ids, recomputed from a plugin folder's files.
//!
//! The catalog pins a plugin's content by the git tree oid of its folder
//! (`treeOid`), because GitHub guarantees stable archive CONTENTS, not stable
//! archive bytes. Rebuilding the oid from what was extracted is the client's
//! proof that it holds exactly the listed files — names, bytes and the
//! executable bit included.

use std::collections::BTreeMap;

use sha1::{Digest, Sha1};
use thiserror::Error;

/// A relative path the builder cannot place in a tree.
#[derive(Debug, Error)]
pub enum TreeError {
    #[error("\"{0}\" is not a relative file path with non-empty components")]
    InvalidPath(String),
}

/// Object id bytes of a git object.
type Oid = [u8; 20];

#[derive(Default)]
struct Dir {
    /// name → (blob oid, executable)
    blobs: BTreeMap<String, (Oid, bool)>,
    dirs: BTreeMap<String, Dir>,
}

/// Accumulates files and hashes them the way `git write-tree` would.
///
/// Directories exist only through the files inside them (git has no empty
/// directories), and modes are exactly `100644`, `100755` and `40000`:
/// symlinks and submodules never reach a listed plugin, the store rejects them.
#[derive(Default)]
pub struct TreeBuilder {
    root: Dir,
}

impl TreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one file. `relative` uses `/` separators and no `.`/`..`.
    pub fn add_blob(
        &mut self,
        relative: &str,
        bytes: &[u8],
        executable: bool,
    ) -> Result<(), TreeError> {
        let mut components: Vec<&str> = relative.split('/').collect();
        let Some(name) = components.pop() else {
            return Err(TreeError::InvalidPath(relative.to_string()));
        };
        if name.is_empty()
            || components
                .iter()
                .chain(std::iter::once(&name))
                .any(|part| part.is_empty() || *part == "." || *part == "..")
        {
            return Err(TreeError::InvalidPath(relative.to_string()));
        }
        let mut dir = &mut self.root;
        for component in components {
            dir = dir.dirs.entry(component.to_string()).or_default();
        }
        dir.blobs
            .insert(name.to_string(), (blob_oid(bytes), executable));
        Ok(())
    }

    /// The tree oid as 40 lowercase hex characters.
    pub fn finish(self) -> String {
        hex(&tree_oid(&self.root))
    }
}

/// `sha1("blob <len>\0" + bytes)`.
pub(crate) fn blob_oid(bytes: &[u8]) -> Oid {
    object_oid("blob", bytes)
}

fn tree_oid(dir: &Dir) -> Oid {
    // Git orders entries by name, comparing a directory as if its name ended
    // in "/": `a.txt` sorts before the directory `a`, which sorts as "a/".
    let mut entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for (name, (oid, executable)) in &dir.blobs {
        let mode: &[u8] = if *executable { b"100755" } else { b"100644" };
        entries.push((name.as_bytes().to_vec(), entry_line(mode, name, oid)));
    }
    for (name, child) in &dir.dirs {
        let mut key = name.as_bytes().to_vec();
        key.push(b'/');
        entries.push((key, entry_line(b"40000", name, &tree_oid(child))));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let body: Vec<u8> = entries.into_iter().flat_map(|(_, line)| line).collect();
    object_oid("tree", &body)
}

fn entry_line(mode: &[u8], name: &str, oid: &Oid) -> Vec<u8> {
    let mut line = Vec::with_capacity(mode.len() + name.len() + 22);
    line.extend_from_slice(mode);
    line.push(b' ');
    line.extend_from_slice(name.as_bytes());
    line.push(0);
    line.extend_from_slice(oid);
    line
}

fn object_oid(kind: &str, body: &[u8]) -> Oid {
    let mut hasher = Sha1::new();
    hasher.update(format!("{kind} {}\0", body.len()).as_bytes());
    hasher.update(body);
    hasher.finalize().into()
}

fn hex(oid: &Oid) -> String {
    oid.iter().map(|byte| format!("{byte:02x}")).collect()
}
