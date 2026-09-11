use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use anyhow::bail;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    FontFaceDescriptor, InstalledGoogleFont, MAX_FACE_DESCRIPTORS, MAX_FONT_BYTES, MAX_FONT_FILES,
    MAX_MANIFEST_BYTES, validate_weight,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CachedManifest {
    pub(super) version: u8,
    pub(super) id: String,
    pub(super) family: String,
    pub(super) faces: Vec<CachedFace>,
    pub(super) files: BTreeMap<String, CachedFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CachedFace {
    pub(super) file: String,
    pub(super) style: String,
    pub(super) weight: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) unicode_range: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CachedFile {
    pub(super) bytes: usize,
    pub(super) sha256: String,
}

impl CachedManifest {
    pub(super) fn into_installed(self, directory: &Path) -> anyhow::Result<InstalledGoogleFont> {
        let faces = self
            .faces
            .into_iter()
            .map(|face| {
                let path = directory.join(&face.file);
                if !path.is_file() {
                    bail!("cached face file is missing");
                }
                Ok(FontFaceDescriptor {
                    family: self.family.clone(),
                    path: path.display().to_string(),
                    style: face.style,
                    weight: face.weight,
                    unicode_range: face.unicode_range,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(InstalledGoogleFont {
            id: self.id,
            family: self.family,
            faces,
        })
    }
}

pub(super) fn load_cached(directory: &Path, id: &str, family: &str) -> Option<InstalledGoogleFont> {
    let manifest_path = directory.join("manifest.json");
    if std::fs::metadata(&manifest_path).ok()?.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    let manifest: CachedManifest =
        serde_json::from_slice(&std::fs::read(&manifest_path).ok()?).ok()?;
    if manifest.version != 1
        || manifest.id != id
        || manifest.family != family
        || manifest.faces.is_empty()
        || manifest.faces.len() > MAX_FACE_DESCRIPTORS
        || manifest.files.is_empty()
        || manifest.files.len() > MAX_FONT_FILES
    {
        return None;
    }
    for (file, expected) in &manifest.files {
        if !safe_cache_file(file) || !(48..=MAX_FONT_BYTES).contains(&expected.bytes) {
            return None;
        }
        let path = directory.join(file);
        let metadata = std::fs::symlink_metadata(&path).ok()?;
        if !metadata.file_type().is_file()
            || metadata.len() != expected.bytes as u64
            || metadata.len() > MAX_FONT_BYTES as u64
        {
            return None;
        }
        let mut bytes = Vec::with_capacity(expected.bytes);
        std::fs::File::open(path)
            .ok()?
            .take((MAX_FONT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() != expected.bytes
            || validate_woff2(&bytes).is_err()
            || sha256(&bytes) != expected.sha256
        {
            return None;
        }
    }
    if manifest.faces.iter().any(|face| {
        !manifest.files.contains_key(&face.file)
            || !matches!(face.style.as_str(), "normal" | "italic")
            || validate_weight(&face.weight).is_err()
    }) {
        return None;
    }
    manifest.into_installed(directory).ok()
}

pub(super) fn safe_cache_file(file: &str) -> bool {
    file.strip_prefix("font-")
        .and_then(|name| name.strip_suffix(".woff2"))
        .is_some_and(|index| index.len() == 3 && index.bytes().all(|byte| byte.is_ascii_digit()))
}

pub(super) fn validate_woff2(bytes: &[u8]) -> anyhow::Result<()> {
    if bytes.len() < 48 || !bytes.starts_with(b"wOF2") {
        bail!("downloaded font is not a WOFF2 file");
    }
    Ok(())
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
