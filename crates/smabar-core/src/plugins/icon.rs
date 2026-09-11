//! Optional folder branding, decoded once with the manifest. Raw files and
//! filesystem paths never reach the webview; every format becomes a small PNG.

use std::fs::{self, File};
use std::io::{self, Cursor, Read};
use std::path::Path;

use base64::Engine;
use image::{ImageFormat, ImageReader, Limits, RgbaImage};
use thiserror::Error;

const ICON_SIZE: u32 = 128;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_DIMENSION: u32 = 2048;
const MAX_DECODE_BYTES: u64 = 32 * 1024 * 1024;
const CANDIDATES: [(&str, ImageFormat); 5] = [
    ("icon.png", ImageFormat::Png),
    ("icon.jpg", ImageFormat::Jpeg),
    ("icon.jpeg", ImageFormat::Jpeg),
    ("icon.ico", ImageFormat::Ico),
    ("icon.webp", ImageFormat::WebP),
];

#[derive(Debug, Error)]
enum IconError {
    #[error("cannot read icon: {0}")]
    Read(#[from] io::Error),
    #[error("icon must be a regular file, not a symlink or directory")]
    NotRegular,
    #[error("icon exceeds the {MAX_FILE_BYTES}-byte file limit")]
    TooLarge,
    #[error("cannot decode or normalize icon: {0}")]
    Image(#[from] image::ImageError),
}

pub(super) fn load(dir: &Path, diagnostics: &mut Vec<String>) -> Option<String> {
    for (name, format) in CANDIDATES {
        let path = dir.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            metadata => metadata,
        };
        let result = metadata.map_err(IconError::from).and_then(|metadata| {
            if !metadata.file_type().is_file() {
                return Err(IconError::NotRegular);
            }
            if metadata.len() > MAX_FILE_BYTES {
                return Err(IconError::TooLarge);
            }
            normalize(&path, format)
        });
        match result {
            Ok(icon) => return Some(icon),
            Err(error) => diagnostics.push(format!(
                "{name} was ignored ({error}); replace it with a valid 128x128 image \
                 of at most {MAX_FILE_BYTES} bytes and {MAX_DIMENSION}px per side"
            )),
        }
    }
    None
}

fn normalize(path: &Path, format: ImageFormat) -> Result<String, IconError> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(IconError::TooLarge);
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let icon = reader.decode()?.thumbnail(ICON_SIZE, ICON_SIZE).to_rgba8();
    let mut canvas = RgbaImage::new(ICON_SIZE, ICON_SIZE);
    image::imageops::replace(
        &mut canvas,
        &icon,
        i64::from((ICON_SIZE - icon.width()) / 2),
        i64::from((ICON_SIZE - icon.height()) / 2),
    );
    let mut png = Cursor::new(Vec::new());
    canvas.write_to(&mut png, ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png.into_inner())
    ))
}

#[cfg(test)]
#[path = "icon_tests.rs"]
mod tests;
