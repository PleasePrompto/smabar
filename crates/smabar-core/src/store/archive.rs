//! Bounded, path-safe extraction of one plugin folder out of a GitHub commit
//! archive, hashing the files on the way out.
//!
//! A commit archive is the whole repository under one top-level folder
//! `<repo>-<commit>/`; only the entries below the listed plugin path are
//! written. Nothing is executed, every path is checked before it is used, and
//! the git tree oid is computed from the archive's own bytes and modes — the
//! filesystem may lose the executable bit (Windows) but the proof does not.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, Cursor, Read, Seek};
use std::path::{Path, PathBuf};

use thiserror::Error;
use zip::ZipArchive;

use super::treeoid::{TreeBuilder, TreeError};

/// Caps that keep a hostile or bloated archive from filling the disk.
#[derive(Debug, Clone, Copy)]
pub struct ArchiveLimits {
    pub max_files: usize,
    pub max_directories: usize,
    pub max_depth: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}

impl ArchiveLimits {
    /// The store lists folders of at most 200 directories; the rest is
    /// generous for a text plugin and hostile to a repository-as-CDN.
    pub const DEFAULT: Self = Self {
        max_files: 5_000,
        max_directories: 200,
        max_depth: 16,
        max_file_bytes: 16 * 1024 * 1024,
        max_total_bytes: 200 * 1024 * 1024,
    };
}

/// What came out of the archive.
#[derive(Debug)]
pub struct Extracted {
    /// Git tree oid of the extracted folder, from the archive's bytes.
    pub tree_oid: String,
    pub files: usize,
    pub bytes: u64,
    /// The bytes of `smabar.json` at the folder root.
    pub manifest: Vec<u8>,
}

/// Why an archive was refused; nothing has been swapped into place when
/// one of these is returned.
#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("cannot open {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not a zip archive: {source}")]
    NotAZip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error(
        "archive entry \"{name}\" lies outside \"{root}\"; this is not GitHub's archive of that commit"
    )]
    ForeignEntry { name: String, root: String },
    #[error(
        "the archive holds no \"{prefix}smabar.json\"; the listed folder is not in this commit"
    )]
    NoManifest { prefix: String },
    #[error(
        "archive entry \"{0}\" has an unsafe path (absolute, \"..\", or a reserved name); refusing it"
    )]
    UnsafePath(String),
    #[error("archive entry \"{0}\" is a symlink; symlinks cannot be installed")]
    Symlink(String),
    #[error("archive entry \"{0}\" collides with another entry once names are case-folded")]
    Duplicate(String),
    #[error("the plugin folder holds more than {0} files; keep plugins small")]
    TooManyFiles(usize),
    #[error("the plugin folder holds more than {0} directories; keep plugins small")]
    TooManyDirectories(usize),
    #[error("archive entry \"{0}\" nests deeper than allowed")]
    TooDeep(String),
    #[error("archive entry \"{name}\" declares {size} bytes, above the {limit}-byte limit")]
    FileTooLarge { name: String, size: u64, limit: u64 },
    #[error("the plugin folder unpacks to more than {0} bytes; refusing it")]
    TotalTooLarge(u64),
    #[error(
        "archive entry \"{0}\" does not hold the byte count it declares; the archive is corrupt"
    )]
    SizeLied(String),
    #[error("cannot read archive entry {name}: {source}")]
    Entry {
        name: String,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("cannot write {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("ZIP must contain exactly one smabar.json at its root or inside one enclosing folder")]
    LocalLayout,
    #[error(transparent)]
    Tree(#[from] TreeError),
}

/// The single top-level folder GitHub puts a commit archive under.
pub fn archive_root(repo_name: &str, commit: &str) -> String {
    format!("{repo_name}-{commit}/")
}

/// Extracts `<repo>-<commit>/<plugin_path>/` into `dest`, which must not
/// exist yet, and returns the proof of what was written.
pub fn extract_plugin(
    zip_path: &Path,
    repo_name: &str,
    commit: &str,
    plugin_path: &str,
    dest: &Path,
    limits: &ArchiveLimits,
) -> Result<Extracted, ArchiveError> {
    let root = archive_root(repo_name, commit);
    let prefix = if plugin_path == "." {
        root.clone()
    } else {
        format!("{root}{}/", plugin_path.trim_matches('/'))
    };
    let file = File::open(zip_path).map_err(|source| ArchiveError::Open {
        path: zip_path.to_path_buf(),
        source,
    })?;
    let archive =
        ZipArchive::new(BufReader::new(file)).map_err(|source| ArchiveError::NotAZip {
            path: zip_path.to_path_buf(),
            source,
        })?;
    extract_archive(archive, &root, &prefix, dest, limits)
}

/// A local archive contains one plugin, flat or inside one enclosing folder.
pub fn extract_local_plugin(
    bytes: &[u8],
    dest: &Path,
    limits: &ArchiveLimits,
) -> Result<Extracted, ArchiveError> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|source| ArchiveError::NotAZip {
            path: PathBuf::from("selected ZIP"),
            source,
        })?;
    if archive.len() > limits.max_files + limits.max_directories {
        return Err(ArchiveError::TooManyFiles(limits.max_files));
    }
    let mut roots = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|source| ArchiveError::Entry {
                name: format!("#{index}"),
                source,
            })?;
        let name = entry.name();
        if name == crate::plugins::MANIFEST_FILE {
            roots.push(String::new());
        } else if let Some(prefix) = name.strip_suffix("/smabar.json") {
            if prefix.contains('/') {
                return Err(ArchiveError::LocalLayout);
            }
            roots.push(format!("{prefix}/"));
        }
    }
    if roots.len() != 1 {
        return Err(ArchiveError::LocalLayout);
    }
    extract_archive(archive, "", &roots[0], dest, limits)
}

fn extract_archive<R: Read + Seek>(
    mut archive: ZipArchive<R>,
    root: &str,
    prefix: &str,
    dest: &Path,
    limits: &ArchiveLimits,
) -> Result<Extracted, ArchiveError> {
    fs::create_dir_all(dest).map_err(|source| ArchiveError::Io {
        path: dest.to_path_buf(),
        source,
    })?;

    let mut tree = TreeBuilder::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut directories: HashSet<String> = HashSet::new();
    let mut files = 0usize;
    let mut total = 0u64;
    let mut manifest = None;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|source| ArchiveError::Entry {
                name: format!("#{index}"),
                source,
            })?;
        let name = entry.name().to_string();
        if !name.starts_with(root) {
            return Err(ArchiveError::ForeignEntry {
                name,
                root: root.to_string(),
            });
        }
        if !is_safe_relative(name.trim_end_matches('/')) || entry.enclosed_name().is_none() {
            return Err(ArchiveError::UnsafePath(name));
        }
        if entry.is_symlink() {
            return Err(ArchiveError::Symlink(name));
        }
        if entry.is_dir() {
            continue;
        }
        let Some(relative) = name.strip_prefix(prefix) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        let relative = relative.to_string();
        if entry.enclosed_name().is_none() || !is_safe_relative(&relative) {
            return Err(ArchiveError::UnsafePath(name));
        }
        if entry.is_symlink() {
            return Err(ArchiveError::Symlink(name));
        }
        if relative.matches('/').count() + 1 > limits.max_depth {
            return Err(ArchiveError::TooDeep(name));
        }
        if !seen.insert(relative.to_lowercase()) {
            return Err(ArchiveError::Duplicate(name));
        }
        files += 1;
        if files > limits.max_files {
            return Err(ArchiveError::TooManyFiles(limits.max_files));
        }
        let mut parent = relative.as_str();
        while let Some((directory, _)) = parent.rsplit_once('/') {
            if directories.insert(directory.to_string())
                && directories.len() > limits.max_directories
            {
                return Err(ArchiveError::TooManyDirectories(limits.max_directories));
            }
            parent = directory;
        }
        let size = entry.size();
        if size > limits.max_file_bytes {
            return Err(ArchiveError::FileTooLarge {
                name,
                size,
                limit: limits.max_file_bytes,
            });
        }
        total = total.saturating_add(size);
        if total > limits.max_total_bytes {
            return Err(ArchiveError::TotalTooLarge(limits.max_total_bytes));
        }
        // git archive records 0755 only for executable files; every other
        // entry has no unix mode at all, which is a plain 0644.
        let executable = entry.unix_mode().is_some_and(|mode| mode & 0o100 != 0);
        let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
        (&mut entry)
            .take(size + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| ArchiveError::Io {
                path: PathBuf::from(&name),
                source,
            })?;
        if bytes.len() as u64 != size {
            return Err(ArchiveError::SizeLied(name));
        }
        let target = dest.join(&relative);
        write_file(&target, &bytes, executable)?;
        tree.add_blob(&relative, &bytes, executable)?;
        if relative == crate::plugins::MANIFEST_FILE {
            manifest = Some(bytes);
        }
    }

    let manifest = manifest.ok_or(ArchiveError::NoManifest {
        prefix: prefix.to_string(),
    })?;
    Ok(Extracted {
        tree_oid: tree.finish(),
        files,
        bytes: total,
        manifest,
    })
}

fn write_file(target: &Path, bytes: &[u8], executable: bool) -> Result<(), ArchiveError> {
    let io_error = |source| ArchiveError::Io {
        path: target.to_path_buf(),
        source,
    };
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::write(target, bytes).map_err(io_error)?;
    if executable {
        crate::platform::mark_executable(target).map_err(io_error)?;
    }
    Ok(())
}

/// A relative path every platform can create and no platform can abuse:
/// plain components, no traversal, no separators or drive syntax inside a
/// component, and none of the names Windows refuses to create.
fn is_safe_relative(relative: &str) -> bool {
    relative.split('/').all(|component| {
        !component.is_empty()
            && component != "."
            && component != ".."
            && !component.contains(['\\', ':', '\0'])
            && !component.chars().any(char::is_control)
            && !component.ends_with(['.', ' '])
            && !is_windows_reserved(component)
    })
}

fn is_windows_reserved(component: &str) -> bool {
    let base = component
        .split('.')
        .next()
        .unwrap_or(component)
        .to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit()
            && base.as_bytes()[3] != b'0')
}
