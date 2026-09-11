//! Pure crop and PNG transforms shared by the platform snapshot backends.

#[cfg(any(windows, test))]
use std::io::Cursor;

use smabar_core::capture::{BarError, MAX_CAPTURE_EDGE};
use smabar_core::platform::Rect;

/// Large enough for an 8K viewport while bounding the decoded source buffer.
#[cfg(any(windows, test))]
pub(super) const MAX_DECODE_PIXELS: u64 = 64_000_000;

/// Where in a rendered surface the CSS-pixel subject sits, in device pixels.
/// The result is clipped to the surface; `None` means there is no overlap.
pub(super) fn device_crop(
    rect: Rect,
    scale_factor: f64,
    surface: (i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    let to_device = |value: f64| (value * scale_factor).round() as i32;
    let (surface_w, surface_h) = surface;
    let left = to_device(f64::from(rect.x)).max(0);
    let top = to_device(f64::from(rect.y)).max(0);
    let right = to_device(f64::from(rect.x) + f64::from(rect.w)).min(surface_w);
    let bottom = to_device(f64::from(rect.y) + f64::from(rect.h)).min(surface_h);
    if right <= left || bottom <= top {
        return None;
    }
    Some((left, top, right - left, bottom - top))
}

/// Output size for a crop at `scale`, refused before any large allocation.
pub(super) fn scaled_size(width: i32, height: i32, scale: u8) -> Result<(i32, i32), BarError> {
    let factor = i32::from(scale.max(1));
    let out_w = width.checked_mul(factor).unwrap_or(i32::MAX);
    let out_h = height.checked_mul(factor).unwrap_or(i32::MAX);
    let edge = u32::try_from(out_w.max(out_h)).unwrap_or(u32::MAX);
    if edge > MAX_CAPTURE_EDGE {
        return Err(BarError::Failed(format!(
            "a {out_w}x{out_h} capture is too large (limit {MAX_CAPTURE_EDGE} px per edge) — \
             lower `scale` or pick a smaller target"
        )));
    }
    Ok((out_w, out_h))
}

/// Crops a natural-size PNG and magnifies it with nearest-neighbour sampling.
/// The decoded color type is kept, including an alpha channel when present.
#[cfg(any(windows, test))]
pub(super) fn crop_scale_png(
    bytes: &[u8],
    rect: Rect,
    scale_factor: f64,
    scale: u8,
) -> Result<(Vec<u8>, u32, u32), BarError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|error| BarError::Failed(format!("cannot decode the webview PNG: {error}")))?;
    let source_info = reader.info();
    validate_decode_size(source_info.width, source_info.height)?;
    let decoded_len = reader.output_buffer_size().ok_or_else(|| {
        BarError::Failed("the decoded webview PNG is too large for this process".into())
    })?;
    let mut decoded = zeroed(decoded_len)?;
    let info = reader
        .next_frame(&mut decoded)
        .map_err(|error| BarError::Failed(format!("cannot read the webview PNG: {error}")))?;
    decoded.truncate(info.buffer_size());
    if info.bit_depth != png::BitDepth::Eight {
        return Err(BarError::Failed(format!(
            "the webview PNG decoded to unsupported {:?} samples",
            info.bit_depth
        )));
    }

    let surface_w = i32::try_from(info.width)
        .map_err(|_| BarError::Failed("the webview PNG is wider than supported".into()))?;
    let surface_h = i32::try_from(info.height)
        .map_err(|_| BarError::Failed("the webview PNG is taller than supported".into()))?;
    let (x, y, width, height) = device_crop(rect, scale_factor, (surface_w, surface_h))
        .ok_or_else(|| {
            BarError::Failed(
                "the subject sits outside the rendered viewport — it is probably not visible"
                    .into(),
            )
        })?;
    let (out_w, out_h) = scaled_size(width, height, scale)?;
    let factor = usize::from(scale.max(1));
    let channels = info.color_type.samples();
    let out_w_usize = usize::try_from(out_w).map_err(invalid_image_size)?;
    let out_h_usize = usize::try_from(out_h).map_err(invalid_image_size)?;
    let output_len = out_w_usize
        .checked_mul(out_h_usize)
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or_else(|| BarError::Failed("the scaled webview PNG is too large".into()))?;
    let mut output = zeroed(output_len)?;
    let x = usize::try_from(x).map_err(invalid_image_size)?;
    let y = usize::try_from(y).map_err(invalid_image_size)?;

    for out_y in 0..out_h_usize {
        let source_y = y + out_y / factor;
        for out_x in 0..out_w_usize {
            let source_x = x + out_x / factor;
            let source = source_y
                .checked_mul(info.line_size)
                .and_then(|row| {
                    source_x
                        .checked_mul(channels)
                        .and_then(|column| row.checked_add(column))
                })
                .ok_or_else(|| BarError::Failed("the webview PNG row is too large".into()))?;
            let target = (out_y * out_w_usize + out_x) * channels;
            let source_pixel = decoded.get(source..source + channels).ok_or_else(|| {
                BarError::Failed("the decoded webview PNG ended inside a pixel".into())
            })?;
            output[target..target + channels].copy_from_slice(source_pixel);
        }
    }

    let out_w = u32::try_from(out_w).map_err(invalid_image_size)?;
    let out_h = u32::try_from(out_h).map_err(invalid_image_size)?;
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, out_w, out_h);
        encoder.set_color(info.color_type);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| BarError::Failed(format!("cannot encode the capture PNG: {error}")))?;
        writer
            .write_image_data(&output)
            .map_err(|error| BarError::Failed(format!("cannot write the capture PNG: {error}")))?;
    }
    Ok((png, out_w, out_h))
}

#[cfg(any(windows, test))]
pub(super) fn zeroed(length: usize) -> Result<Vec<u8>, BarError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|error| BarError::Failed(format!("cannot allocate the capture: {error}")))?;
    bytes.resize(length, 0);
    Ok(bytes)
}

#[cfg(any(windows, test))]
fn invalid_image_size(error: std::num::TryFromIntError) -> BarError {
    BarError::Failed(format!("the webview PNG has an invalid size: {error}"))
}

#[cfg(any(windows, test))]
fn validate_decode_size(width: u32, height: u32) -> Result<(), BarError> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| BarError::Failed("the webview PNG dimensions overflow".into()))?;
    if pixels > MAX_DECODE_PIXELS {
        return Err(BarError::Failed(format!(
            "the webview PNG is {width}x{height} px ({pixels} pixels; decode limit \
             {MAX_DECODE_PIXELS})"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn maps_css_pixels_to_device_pixels() {
        let crop = device_crop(rect(10, 20, 100, 50), 2.0, (2560, 1560)).expect("on surface");
        assert_eq!(crop, (20, 40, 200, 100));
    }

    #[test]
    fn passes_through_at_scale_factor_one() {
        let crop = device_crop(rect(0, 1400, 2560, 80), 1.0, (2560, 1560)).expect("on surface");
        assert_eq!(crop, (0, 1400, 2560, 80));
    }

    #[test]
    fn clips_to_the_surface() {
        let crop = device_crop(rect(-20, 1500, 200, 400), 1.0, (2560, 1560)).expect("overlaps");
        assert_eq!(crop, (0, 1500, 180, 60));
    }

    #[test]
    fn rejects_a_subject_entirely_off_surface() {
        assert!(device_crop(rect(0, 2000, 100, 50), 1.0, (2560, 1560)).is_none());
        assert!(device_crop(rect(3000, 0, 100, 50), 1.0, (2560, 1560)).is_none());
    }

    #[test]
    fn multiplies_by_scale() {
        assert_eq!(scaled_size(120, 40, 3).expect("in range"), (360, 120));
        assert_eq!(scaled_size(120, 40, 0).expect("in range"), (120, 40));
    }

    #[test]
    fn refuses_an_oversized_capture() {
        let error = scaled_size(2560, 80, 4).expect_err("over the edge limit");
        assert!(matches!(error, BarError::Failed(_)));
    }

    #[test]
    fn decode_budget_allows_8k_but_rejects_more_than_64_megapixels() {
        validate_decode_size(7680, 4320).expect("8K fits the decode budget");
        let error = validate_decode_size(8001, 8000).expect_err("over the pixel budget");
        assert!(matches!(error, BarError::Failed(_)));
    }

    #[test]
    fn png_crop_uses_nearest_and_keeps_alpha() {
        let source = encode_rgba(2, 1, &[255, 0, 0, 0, 0, 0, 255, 127]);
        let (result, width, height) =
            crop_scale_png(&source, rect(1, 0, 1, 1), 1.0, 2).expect("crop");
        assert_eq!((width, height), (2, 2));

        let decoder = png::Decoder::new(Cursor::new(result));
        let mut reader = decoder.read_info().expect("header");
        let mut pixels = vec![0; reader.output_buffer_size().expect("buffer size")];
        let info = reader.next_frame(&mut pixels).expect("frame");
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(
            &pixels[..info.buffer_size()],
            &[
                0, 0, 255, 127, 0, 0, 255, 127, 0, 0, 255, 127, 0, 0, 255, 127
            ]
        );
    }

    #[test]
    fn png_crop_keeps_rgb_without_adding_alpha() {
        let source = encode(png::ColorType::Rgb, 2, 1, &[255, 0, 0, 0, 0, 255]);
        let (result, width, height) =
            crop_scale_png(&source, rect(0, 0, 1, 1), 1.0, 1).expect("crop");
        assert_eq!((width, height), (1, 1));

        let decoder = png::Decoder::new(Cursor::new(result));
        let mut reader = decoder.read_info().expect("header");
        let mut pixels = vec![0; reader.output_buffer_size().expect("buffer size")];
        let info = reader.next_frame(&mut pixels).expect("frame");
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(&pixels[..info.buffer_size()], &[255, 0, 0]);
    }

    fn encode_rgba(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        encode(png::ColorType::Rgba, width, height, pixels)
    }

    fn encode(color_type: png::ColorType, width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(color_type);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(pixels).expect("pixels");
        }
        bytes
    }
}
