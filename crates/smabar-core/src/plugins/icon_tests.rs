use super::*;
use crate::plugins::PluginManifest;
use image::{DynamicImage, Rgba};

fn write_image(path: &Path, format: ImageFormat, width: u32, height: u32, alpha: u8) {
    let rgba = RgbaImage::from_pixel(width, height, Rgba([245, 196, 0, alpha]));
    let image = if format == ImageFormat::Jpeg {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).to_rgb8())
    } else {
        DynamicImage::ImageRgba8(rgba)
    };
    image.save_with_format(path, format).expect("write icon");
}

fn decoded(data_url: &str) -> RgbaImage {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            data_url
                .strip_prefix("data:image/png;base64,")
                .expect("PNG URL"),
        )
        .expect("base64");
    image::load_from_memory(&bytes)
        .expect("normalized PNG")
        .to_rgba8()
}

#[test]
fn every_supported_format_becomes_a_128px_png_with_its_alpha_intact() {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, format) in CANDIDATES {
        let path = dir.path().join(name);
        write_image(&path, format, 128, 128, 96);
        let mut diagnostics = Vec::new();
        let icon = decoded(&load(dir.path(), &mut diagnostics).expect(name));
        assert_eq!(icon.dimensions(), (128, 128), "{name}");
        assert_eq!(
            icon.get_pixel(64, 64)[3],
            if format == ImageFormat::Jpeg { 255 } else { 96 }
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        fs::remove_file(path).expect("remove icon");
    }
}

#[test]
fn folder_priority_reload_removal_and_aspect_ratio_are_predictable() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("smabar.json"),
        r#"{
        "id":"demo", "name":"Demo", "version":"1.0.0", "protocolVersion":1,
        "runtime":"exec", "command":["demo"], "tiles":[{"id":"main","name":"Demo"}]
    }"#,
    )
    .expect("manifest");
    assert!(
        PluginManifest::load(dir.path())
            .expect("no icon")
            .icon_data_url
            .is_none()
    );
    write_image(
        &dir.path().join("icon.webp"),
        ImageFormat::WebP,
        128,
        128,
        255,
    );
    write_image(&dir.path().join("icon.png"), ImageFormat::Png, 256, 128, 96);
    let manifest = PluginManifest::load(dir.path()).expect("load icon");
    let icon = decoded(manifest.icon_data_url.as_deref().expect("icon"));
    assert_eq!(icon.dimensions(), (128, 128));
    assert_eq!(
        icon.get_pixel(64, 0)[3],
        0,
        "transparent padding, no stretching"
    );
    assert_eq!(icon.get_pixel(64, 64)[3], 96, "PNG wins over WebP");
    fs::remove_file(dir.path().join("icon.png")).expect("remove PNG");
    let manifest = PluginManifest::load(dir.path()).expect("reload");
    assert_eq!(
        decoded(manifest.icon_data_url.as_deref().expect("WebP")).get_pixel(64, 0)[3],
        255
    );
    fs::remove_file(dir.path().join("icon.webp")).expect("remove WebP");
    assert!(
        PluginManifest::load(dir.path())
            .expect("removed icon")
            .icon_data_url
            .is_none()
    );
}

#[test]
fn malformed_oversized_and_non_file_icons_warn_and_try_the_next_format() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_image(
        &dir.path().join("icon.webp"),
        ImageFormat::WebP,
        128,
        128,
        255,
    );
    let png = dir.path().join("icon.png");
    for invalid in [
        b"<svg onload='bad()'/>".to_vec(),
        vec![0; MAX_FILE_BYTES as usize + 1],
    ] {
        fs::write(&png, invalid).expect("invalid input");
        let mut diagnostics = Vec::new();
        assert!(
            load(dir.path(), &mut diagnostics).is_some(),
            "WebP fallback"
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("icon.png was ignored"));
    }
    write_image(&png, ImageFormat::Png, MAX_DIMENSION + 1, 1, 255);
    let mut diagnostics = Vec::new();
    assert!(load(dir.path(), &mut diagnostics).is_some());
    assert_eq!(diagnostics.len(), 1, "dimensions limited before resizing");
    fs::remove_file(&png).expect("remove PNG");
    fs::create_dir(&png).expect("directory is not an image");
    diagnostics.clear();
    assert!(load(dir.path(), &mut diagnostics).is_some());
    assert!(diagnostics[0].contains("regular file"));
    fs::remove_file(dir.path().join("icon.webp")).expect("remove fallback");
    assert!(load(dir.path(), &mut Vec::new()).is_none());
}
