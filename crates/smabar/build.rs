mod build_extension;
mod build_focus_grab;

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // externalBin is a release artifact. Tauri validates it in build.rs, so
    // remove it from debug/check config; release builds fail if fetch-uv.sh
    // was not run first.
    if std::env::var("PROFILE").as_deref() != Ok("release")
        && std::env::var_os("TAURI_CONFIG").is_none()
    {
        // SAFETY: Cargo runs this build script single-threaded and no threads
        // have been spawned before this process-local configuration update.
        unsafe {
            std::env::set_var("TAURI_CONFIG", r#"{"bundle":{"externalBin":null}}"#);
        }
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        build_focus_grab::build().unwrap_or_else(|error| {
            panic!("failed to build Wayland outside-click support: {error}")
        });
        build_web_extension().unwrap_or_else(|error| {
            panic!("failed to build the WebKit provider-identity extension: {error}")
        });
    }
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "set_input_shape",
            "get_ui_state",
            "plugin_action",
            "get_plugins",
            "get_plugin_ui",
            "take_plugin_ui",
            "list_plugins",
            "remove_plugin",
            "set_bar_geometry",
            "set_bar_revealed",
            "update_config",
            "list_themes",
            "get_system_settings",
            "get_autostart_status",
            "set_autostart",
            "get_reservation_status",
            "request_reservation_access",
            "save_custom_theme",
            "delete_theme",
            "export_theme",
            "import_theme",
            "get_theme_export_dir",
            "font_list",
            "ensure_google_font",
            "list_apps",
            "get_app_icon",
            "get_shortcuts",
            "pin_shortcut",
            "pin_special_shortcut",
            "unpin_shortcut",
            "launch_shortcut",
            "get_surface_context",
            "get_monitor_state",
            "surface_ready",
            "open_settings",
            "toggle_settings",
            "close_settings",
            "close_surface",
            "show_notice",
            "set_notification_measure",
            "stage_notification_update",
            "open_flyout",
            "measure_flyout",
            "pin_flyout",
            "close_flyout",
            "finalize_overlay_clear",
            "set_overlay_pointer",
            "open_context_menu",
            "measure_context_menu",
            "close_context_menu",
            "open_tooltip",
            "measure_tooltip",
            "close_tooltip",
            "focus_bar",
            "open_url",
            "ui_log",
            "get_managed_popups",
            "popup_event_report",
            "get_audio_settings",
            "get_runtime_status",
            "retry_provisioning",
            "check_update",
            "install_update",
            "store_overview",
            "store_refresh",
            "store_detail",
            "store_install_plugin",
            "store_install_theme",
            "legal_status",
            "legal_accept",
            "legal_decline",
            "bar_reply",
        ]));
    if let Err(error) = tauri_build::try_build(attributes) {
        // Build scripts can only abort compilation by panicking.
        panic!("tauri-build failed: {error}");
    }
}

fn build_web_extension() -> Result<(), String> {
    const SOURCE: &str = "src/platform/linux/provider_identity.c";
    use build_extension::NAME;

    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    let manifest = PathBuf::from(required_env("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(required_env("OUT_DIR")?);
    let temporary = out_dir.join(NAME);
    let cflags = pkg_config("--cflags")?;
    let libraries = pkg_config("--libs")?;
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let status = Command::new(&compiler)
        .args(["-shared", "-fPIC", "-Os", "-fvisibility=hidden"])
        .args(["-Wall", "-Wextra", "-Werror", "-Wl,--no-undefined"])
        .args(cflags.split_whitespace())
        .arg(manifest.join(SOURCE))
        .arg("-o")
        .arg(&temporary)
        .args(libraries.split_whitespace())
        .status()
        .map_err(|error| format!("cannot run {}: {error}", Path::new(&compiler).display()))?;
    if !status.success() {
        return Err(format!(
            "{} exited with {status}",
            Path::new(&compiler).display()
        ));
    }

    let generated = manifest.join("web-extensions").join(NAME);
    std::fs::create_dir_all(generated.parent().ok_or("invalid extension output path")?)
        .map_err(|error| format!("cannot create extension output directory: {error}"))?;
    let fresh = std::fs::read(&temporary)
        .map_err(|error| format!("cannot read {}: {error}", temporary.display()))?;
    if std::fs::read(&generated).ok().as_deref() != Some(fresh.as_slice()) {
        std::fs::write(&generated, fresh)
            .map_err(|error| format!("cannot write {}: {error}", generated.display()))?;
    }
    build_extension::detach_loaded_extension(&out_dir).map_err(|error| {
        format!("cannot prepare the WebKit extension for Tauri's resource copy: {error}")
    })
}

fn pkg_config(flag: &str) -> Result<String, String> {
    let output = Command::new("pkg-config")
        .args([flag, "webkit2gtk-web-extension-4.1"])
        .output()
        .map_err(|error| format!("cannot run pkg-config: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("pkg-config returned non-UTF-8 output: {error}"))
}

fn required_env(name: &str) -> Result<std::ffi::OsString, String> {
    std::env::var_os(name).ok_or_else(|| format!("Cargo did not set {name}"))
}
