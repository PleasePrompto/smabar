//! Explorer-quality item icons converted from `HBITMAP` to cached PNG.

use std::ffi::c_void;
use std::fs;
use std::mem::size_of;
use std::path::Path;
use std::time::UNIX_EPOCH;

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC,
};
use windows::Win32::UI::Shell::{
    FOLDERID_ComputerFolder, FOLDERID_RecycleBinFolder, IShellItemImageFactory, KF_FLAG_DEFAULT,
    SHCreateItemFromParsingName, SHGetKnownFolderItem, SIIGBF_ICONONLY, SIIGBF_SCALEUP,
};
use windows::core::HSTRING;

use crate::config::SpecialShortcut;
use crate::shortcuts::ShortcutError;
use crate::shortcuts::iconfile::data_uri_for_file;
use crate::shortcuts::platform::IconTarget;

use super::windows_discovery::resolve_link;
use super::windows_error;
use super::windows_model::{icon_cache_name, normalize_bgra_pixels};

const ICON_SIZE: i32 = 256;

pub(super) fn resolve(
    target: &IconTarget,
    icons_dir: &Path,
) -> Result<Option<String>, ShortcutError> {
    let (key, fallback) = match target {
        IconTarget::Path(path) => {
            let fallback = is_link(path).then(|| resolve_link(path).ok()).flatten();
            let mut key = format!("path:{}:{}", path.display(), fingerprint(path));
            if let Some(info) = &fallback {
                key.push_str(&format!(
                    ":{}:{}",
                    info.target.display(),
                    fingerprint(&info.target)
                ));
            }
            (key, fallback.map(|info| info.target))
        }
        IconTarget::Special(special) => (format!("special:{special:?}"), None),
        IconTarget::Theme(_) => return Ok(None),
    };
    let cache = icons_dir.join(icon_cache_name(&key));
    if cache.is_file() {
        return Ok(data_uri_for_file(&cache));
    }
    fs::create_dir_all(icons_dir).map_err(|source| ShortcutError::Platform {
        action: "create the native shortcut icon cache",
        source,
    })?;

    let bitmap = match target {
        IconTarget::Path(path) => image_for_path(path)
            .or_else(|first_error| fallback.as_deref().map_or(Err(first_error), image_for_path))?,
        IconTarget::Special(special) => image_for_special(*special)?,
        IconTarget::Theme(_) => return Ok(None),
    };
    let (width, height, pixels) = bitmap_pixels(bitmap)?;
    let png = encode_png(width, height, &pixels)?;
    write_atomically(&cache, &png).map_err(|source| ShortcutError::Platform {
        action: "cache a native shortcut icon",
        source,
    })?;
    tracing::debug!(path = %cache.display(), width, height, "cached native shortcut icon");
    Ok(data_uri_for_file(&cache))
}

fn image_for_path(path: &Path) -> Result<HBITMAP, ShortcutError> {
    let parsing_name = HSTRING::from(path.as_os_str());
    // SAFETY: called only on the shell STA; the COM object and HBITMAP remain
    // worker-local and the bitmap is released by `bitmap_pixels`.
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&parsing_name, None)
            .map_err(|error| windows_error("create an Explorer icon factory", error))?;
        factory
            .GetImage(
                SIZE {
                    cx: ICON_SIZE,
                    cy: ICON_SIZE,
                },
                SIIGBF_ICONONLY | SIIGBF_SCALEUP,
            )
            .map_err(|error| windows_error("extract an Explorer item icon", error))
    }
}

fn image_for_special(special: SpecialShortcut) -> Result<HBITMAP, ShortcutError> {
    let id = match special {
        SpecialShortcut::Computer => &FOLDERID_ComputerFolder,
        SpecialShortcut::Trash => &FOLDERID_RecycleBinFolder,
    };
    // SAFETY: called on the shell STA; the known-folder item stays local.
    unsafe {
        let factory: IShellItemImageFactory = SHGetKnownFolderItem(id, KF_FLAG_DEFAULT, None)
            .map_err(|error| windows_error("resolve a Windows special-item icon", error))?;
        factory
            .GetImage(
                SIZE {
                    cx: ICON_SIZE,
                    cy: ICON_SIZE,
                },
                SIIGBF_ICONONLY | SIIGBF_SCALEUP,
            )
            .map_err(|error| windows_error("extract a Windows special-item icon", error))
    }
}

fn bitmap_pixels(bitmap: HBITMAP) -> Result<(u32, u32, Vec<u8>), ShortcutError> {
    let bitmap = OwnedBitmap(bitmap);
    let mut info = BITMAP::default();
    let object_size = i32::try_from(size_of::<BITMAP>())
        .map_err(|error| invalid_bitmap("convert the BITMAP structure size", error.to_string()))?;
    // SAFETY: `info` is valid writable storage and the owned HBITMAP remains
    // alive through this function.
    if unsafe {
        GetObjectW(
            bitmap.0.into(),
            object_size,
            Some((&raw mut info).cast::<c_void>()),
        )
    } == 0
    {
        return Err(invalid_bitmap(
            "read native icon bitmap metadata",
            "GetObjectW returned zero",
        ));
    }
    let width = u32::try_from(info.bmWidth)
        .map_err(|error| invalid_bitmap("read native icon width", error.to_string()))?;
    let height = u32::try_from(info.bmHeight)
        .map_err(|error| invalid_bitmap("read native icon height", error.to_string()))?;
    let byte_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| invalid_bitmap("allocate native icon pixels", "bitmap is too large"))?;
    let header_size = u32::try_from(size_of::<BITMAPINFOHEADER>()).map_err(|error| {
        invalid_bitmap(
            "convert the BITMAPINFOHEADER structure size",
            error.to_string(),
        )
    })?;

    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: header_size,
            biWidth: info.bmWidth,
            biHeight: -info.bmHeight,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut pixels = vec![0_u8; byte_count];
    // SAFETY: the DC is released by its guard; the pixel buffer and bitmap
    // info describe exactly width*height 32-bit pixels.
    let dc = OwnedDc(unsafe { CreateCompatibleDC(None) });
    if dc.0.is_invalid() {
        return Err(invalid_bitmap(
            "create a native icon device context",
            "CreateCompatibleDC returned an invalid handle",
        ));
    }
    let rows = unsafe {
        GetDIBits(
            dc.0,
            bitmap.0,
            0,
            height,
            Some(pixels.as_mut_ptr().cast::<c_void>()),
            &raw mut header,
            DIB_RGB_COLORS,
        )
    };
    if rows == 0 {
        return Err(invalid_bitmap(
            "read native icon pixels",
            "GetDIBits returned zero rows",
        ));
    }
    normalize_bgra_pixels(&mut pixels);
    Ok((width, height, pixels))
}

fn encode_png(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, ShortcutError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| ShortcutError::Platform {
                action: "encode a native shortcut icon as PNG",
                source: std::io::Error::other(error),
            })?;
        writer
            .write_image_data(pixels)
            .map_err(|error| ShortcutError::Platform {
                action: "write native shortcut icon PNG pixels",
                source: std::io::Error::other(error),
            })?;
    }
    Ok(bytes)
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("part");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

fn fingerprint(path: &Path) -> String {
    let Ok(metadata) = fs::metadata(path) else {
        return "missing".to_string();
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0_u128, |duration| duration.as_nanos());
    format!("{}:{modified}", metadata.len())
}

fn is_link(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
}

fn invalid_bitmap(action: &'static str, message: impl Into<String>) -> ShortcutError {
    ShortcutError::Platform {
        action,
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()),
    }
}

struct OwnedBitmap(HBITMAP);

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        // SAFETY: GetImage transfers one HBITMAP that this guard owns.
        let _ = unsafe { DeleteObject(self.0.into()) };
    }
}

struct OwnedDc(HDC);

impl Drop for OwnedDc {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: this guard owns the compatible DC.
            let _ = unsafe { DeleteDC(self.0) };
        }
    }
}
