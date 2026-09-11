use anyhow::Context;
use tauri::WebviewWindow;

pub fn normalize(window: &WebviewWindow, device_pixel_ratio: f64) -> anyhow::Result<()> {
    let native_scale = window
        .scale_factor()
        .context("failed to read the native window scale")?;
    let zoom = compensating_zoom(native_scale, device_pixel_ratio)?;
    if (zoom - 1.0).abs() < 0.01 {
        return Ok(());
    }
    window
        .set_zoom(zoom)
        .context("failed to align WebKit and native window scaling")?;
    tracing::info!(
        surface = window.label(),
        native_scale,
        device_pixel_ratio,
        zoom,
        "normalized WebKit surface scale"
    );
    Ok(())
}

fn compensating_zoom(native_scale: f64, device_pixel_ratio: f64) -> anyhow::Result<f64> {
    if !native_scale.is_finite()
        || native_scale <= 0.0
        || !device_pixel_ratio.is_finite()
        || device_pixel_ratio <= 0.0
    {
        anyhow::bail!("window scale and devicePixelRatio must be positive finite numbers");
    }
    Ok(native_scale / device_pixel_ratio)
}

#[cfg(test)]
mod tests {
    use super::compensating_zoom;

    #[test]
    fn cancels_only_the_webview_native_scale_mismatch() {
        assert_eq!(compensating_zoom(1.0, 2.0).unwrap(), 0.5);
        assert_eq!(compensating_zoom(2.0, 2.0).unwrap(), 1.0);
        assert!(compensating_zoom(1.0, 0.0).is_err());
    }
}
