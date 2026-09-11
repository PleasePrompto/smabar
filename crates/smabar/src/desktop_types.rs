//! Wire types shared by desktop RPC, audio and notification presentation.
use serde::{Deserialize, Serialize};
use std::path::{Component, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetRoot {
    Plugin,
    Data,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSource {
    pub root: AssetRoot,
    pub path: String,
}

impl AudioSource {
    pub fn resolve(
        &self,
        plugin_dir: &std::path::Path,
        data_dir: &std::path::Path,
    ) -> anyhow::Result<PathBuf> {
        if self.path.is_empty()
            || self.path.contains(['\\', ':', '\0'])
            || !std::path::Path::new(&self.path)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
        {
            anyhow::bail!(
                "audio source must be a relative path inside this plugin's code or data folder"
            );
        }
        let root = match self.root {
            AssetRoot::Plugin => plugin_dir,
            AssetRoot::Data => data_dir,
        };
        let canonical_root = root
            .canonicalize()
            .map_err(|_| anyhow::anyhow!("audio source folder is unavailable"))?;
        let file = canonical_root
            .join(&self.path)
            .canonicalize()
            .map_err(|_| anyhow::anyhow!("audio source file does not exist"))?;
        if !file.starts_with(&canonical_root) || !file.is_file() {
            anyhow::bail!(
                "audio source must stay inside this plugin's folder and be a regular file"
            );
        }
        Ok(file)
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

pub fn volume_default() -> u8 {
    100
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioPlay {
    pub playback_id: String,
    pub source: AudioSource,
    #[serde(default = "volume_default")]
    pub volume: u8,
    #[serde(default, rename = "loop")]
    pub looping: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PopupShow {
    pub tile_id: String,
    pub popup_id: String,
    pub html: String,
    #[serde(default)]
    pub ttl_ms: Option<u32>,
    #[serde(default)]
    pub sound: Option<AudioSource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PopupId {
    pub tile_id: String,
    pub popup_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_audio_resolves_only_regular_files_in_the_selected_root() {
        let folder = tempfile::tempdir().expect("folder");
        let code = folder.path().join("code");
        let data = folder.path().join("data");
        std::fs::create_dir_all(&code).expect("code");
        std::fs::create_dir_all(&data).expect("data");
        std::fs::write(code.join("tone.wav"), b"test").expect("tone");
        let source = |path: &str| AudioSource {
            root: AssetRoot::Plugin,
            path: path.into(),
        };
        // Compare canonical forms: on Windows the temp dir may be handed out as
        // an 8.3 short name (`MARKET~1`) while `resolve` returns the long form.
        assert_eq!(
            source("tone.wav").resolve(&code, &data).expect("file"),
            code.join("tone.wav")
                .canonicalize()
                .expect("canonical tone path")
        );
        for path in [
            "../tone.wav",
            "/etc/passwd",
            "C:\\file.wav",
            "https://site/audio",
            "",
            "folder/../tone.wav",
            ".",
            "missing.wav",
        ] {
            assert!(source(path).resolve(&code, &data).is_err(), "{path}");
        }
        assert!(
            AudioSource {
                root: AssetRoot::Data,
                path: "tone.wav".into()
            }
            .resolve(&code, &data)
            .is_err()
        );
    }
}
