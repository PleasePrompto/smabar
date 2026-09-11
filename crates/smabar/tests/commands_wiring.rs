//! A Tauri command only works when THREE lists agree.
//!
//! `main.rs` registers it in `generate_handler!`, `build.rs` declares it in
//! the app manifest, and one of the role capabilities grants its window
//! `allow-<kebab-name>`. Miss one and the command compiles, ships, and then
//! fails at runtime with "not allowed" — which is what happened to
//! `list_plugins`: registered and declared, but never granted.
//!
//! Nothing else catches this. The command functions are unit-testable in
//! isolation and the wiring is data in three separate files, so it is
//! checked here by reading them.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Source text with LF line endings, so the `\n`-joined patterns below
/// also match a CRLF checkout (Windows runners check out with autocrlf).
fn read(relative: &str) -> String {
    let path = crate_dir().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| {
            panic!("cannot read {}: {error}", path.display());
        })
        .replace("\r\n", "\n")
}

/// The command names inside `generate_handler![…]`, without their module path.
fn registered() -> BTreeSet<String> {
    let source = read("src/main.rs");
    let start = source
        .find("generate_handler![")
        .expect("main.rs must register commands with generate_handler!");
    let body = &source[start..];
    let end = body.find(']').expect("unterminated generate_handler!");
    body[..end]
        .lines()
        .skip(1)
        .filter_map(|line| {
            let name = line.trim().trim_end_matches(',').trim();
            let name = name.rsplit("::").next()?;
            (!name.is_empty() && !name.starts_with("//")).then(|| name.to_string())
        })
        .collect()
}

/// The command names in `build.rs`'s app manifest.
fn declared() -> BTreeSet<String> {
    let source = read("build.rs");
    let start = source
        .find(".commands(&[")
        .expect("build.rs must declare the commands");
    let body = &source[start..];
    let end = body.find("])").expect("unterminated commands list");
    body[..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn capabilities() -> Vec<(String, serde_json::Value)> {
    let directory = crate_dir().join("capabilities");
    let mut entries: Vec<_> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()))
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    entries
        .into_iter()
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let source = fs::read_to_string(entry.path()).expect("read capability");
            let value = serde_json::from_str(&source).expect("valid capability JSON");
            (name, value)
        })
        .collect()
}

/// The commands at least one shell role is permitted to invoke.
fn granted() -> BTreeSet<String> {
    capabilities()
        .into_iter()
        .flat_map(|(_, value)| {
            value["permissions"]
                .as_array()
                .expect("permissions must be an array")
                .clone()
        })
        .filter_map(|entry| entry.as_str().map(str::to_string))
        // `core:default` and any other namespaced permission is not one of
        // this app's own commands.
        .filter_map(|entry| entry.strip_prefix("allow-").map(str::to_string))
        .map(|name| name.replace('-', "_"))
        .collect()
}

#[test]
fn every_registered_command_is_declared_and_granted() {
    let registered = registered();
    assert!(
        registered.len() > 10,
        "parsed only {} commands from main.rs — the parser is broken, not the wiring",
        registered.len()
    );
    let declared = declared();
    let granted = granted();

    let undeclared: Vec<_> = registered.difference(&declared).collect();
    assert!(
        undeclared.is_empty(),
        "registered in main.rs but missing from build.rs: {undeclared:?} \
         — the webview gets \"Command not found\""
    );
    let ungranted: Vec<_> = registered.difference(&granted).collect();
    assert!(
        ungranted.is_empty(),
        "registered but not granted in any role capability: {ungranted:?} \
         — the webview gets \"not allowed\", add allow-<kebab-name>"
    );
}

#[test]
fn nothing_is_declared_or_granted_for_a_command_that_does_not_exist() {
    // The other direction: a leftover entry is dead configuration, and a
    // granted permission for a command nobody registered is a stale hole.
    let registered = registered();
    let stale_declared: Vec<_> = declared().difference(&registered).cloned().collect();
    assert!(
        stale_declared.is_empty(),
        "build.rs declares commands main.rs does not register: {stale_declared:?}"
    );
    let stale_granted: Vec<_> = granted().difference(&registered).cloned().collect();
    assert!(
        stale_granted.is_empty(),
        "capabilities grant commands main.rs does not register: {stale_granted:?}"
    );
}

/// Guards the parsers themselves: all three must see the same known command.
#[test]
fn the_parsers_agree_on_a_command_that_is_definitely_wired() {
    for (what, names) in [
        ("main.rs", registered()),
        ("build.rs", declared()),
        ("capabilities", granted()),
    ] {
        assert!(
            names.contains("get_ui_state"),
            "{what} parser found no get_ui_state — it parsed {} names",
            names.len()
        );
    }
}

#[test]
fn every_surface_role_has_one_scoped_capability() {
    let windows: BTreeSet<String> = capabilities()
        .into_iter()
        .flat_map(|(_, value)| {
            value["windows"]
                .as_array()
                .expect("windows must be an array")
                .clone()
        })
        .filter_map(|entry| entry.as_str().map(str::to_string))
        .collect();
    assert_eq!(
        windows,
        ["bar", "notifications", "overlay", "settings"]
            .map(str::to_string)
            .into_iter()
            .collect()
    );
}

#[test]
fn only_settings_can_change_autostart_and_nobody_bypasses_the_app_policy() {
    for (_, capability) in capabilities() {
        let permissions = capability["permissions"].as_array().expect("permissions");
        assert_eq!(
            permissions
                .iter()
                .any(|entry| entry == "allow-set-autostart"),
            capability["identifier"] == "settings"
        );
        assert!(!permissions.iter().any(|entry| {
            entry
                .as_str()
                .is_some_and(|permission| permission.starts_with("autostart:"))
        }));
    }
}

#[test]
fn only_settings_can_accept_or_decline_the_terms() {
    for (_, capability) in capabilities() {
        let permissions = capability["permissions"].as_array().expect("permissions");
        for permission in ["allow-legal-accept", "allow-legal-decline"] {
            assert_eq!(
                permissions.iter().any(|entry| entry == permission),
                capability["identifier"] == "settings",
                "{permission}"
            );
        }
    }
}

#[test]
fn update_surfaces_can_read_the_channel_before_scheduling_checks() {
    for (name, capability) in capabilities() {
        let permissions = capability["permissions"].as_array().expect("permissions");
        let reads_channel = permissions
            .iter()
            .any(|value| value == "allow-get-system-settings");
        assert_eq!(
            reads_channel,
            matches!(name.as_str(), "bar.json" | "settings.json"),
            "{name}"
        );
    }
}

#[test]
fn bar_can_persist_tile_shortcut_and_divider_drags() {
    let (_, bar) = capabilities()
        .into_iter()
        .find(|(_, capability)| capability["identifier"] == "bar")
        .expect("bar capability");
    assert_eq!(bar["windows"], serde_json::json!(["bar"]));
    assert!(
        bar["permissions"]
            .as_array()
            .expect("bar permissions")
            .iter()
            .any(|permission| permission == "allow-update-config"),
        "dragging inside the bar invokes update_config from bar, not overlay or settings"
    );
}

#[test]
fn overlay_can_execute_every_shell_owned_context_menu_command() {
    let (_, overlay) = capabilities()
        .into_iter()
        .find(|(_, capability)| capability["identifier"] == "overlay")
        .expect("overlay capability");
    let permissions: BTreeSet<_> = overlay["permissions"]
        .as_array()
        .expect("permissions must be an array")
        .iter()
        .filter_map(|entry| entry.as_str())
        .collect();

    for permission in [
        "allow-unpin-shortcut",
        "allow-update-config",
        "allow-remove-plugin",
    ] {
        assert!(
            permissions.contains(permission),
            "overlay context menus require {permission}"
        );
    }
}

#[test]
fn asset_protocol_exposes_only_plugin_data_and_cached_google_fonts() {
    let source = read("tauri.conf.json");
    let value: serde_json::Value = serde_json::from_str(&source).expect("valid JSON");
    let scopes: Vec<&str> = value["app"]["security"]["assetProtocol"]["scope"]
        .as_array()
        .expect("asset scope must be an array")
        .iter()
        .filter_map(|entry| entry.as_str())
        .collect();
    assert_eq!(
        scopes,
        [
            "$HOME/.smabar/data/**/*",
            "$HOME/.smabar/cache/fonts/google/**/*",
        ]
    );
}

#[test]
fn remote_frames_never_receive_tauri_capabilities() {
    for (name, capability) in capabilities() {
        assert!(
            capability.get("remote").is_none(),
            "{name}: third-party frames must never receive Tauri command permissions"
        );
    }

    let config: serde_json::Value =
        serde_json::from_str(&read("tauri.conf.json")).expect("valid Tauri config");
    for window in config["app"]["windows"]
        .as_array()
        .expect("app.windows must be an array")
    {
        assert_eq!(
            window["useHttpsScheme"].as_bool(),
            Some(false),
            "the private HTTP embed wrapper would be blocked as mixed content"
        );
    }
    assert_eq!(
        config["app"]["security"]["csp"]["frame-src"], "http://127.0.0.1:*",
        "the privileged shell may frame only the private loopback wrapper"
    );

    let main = read("src/main.rs");
    assert!(
        main.contains("frame_ipc::isolate_subframes(tauri::Builder::default())"),
        "the builder must install the subframe-safe invoke transport"
    );
    let transport = read("src/frame_ipc.rs");
    assert!(
        transport.contains("if (window !== window.top) return")
            && transport.contains("customProtocolIpcBlocked: true")
            && !transport.contains("convertFileSrc(cmd"),
        "remote WebView2 frames must neither invoke commands nor reach Tauri's channel queue"
    );
}

#[test]
fn every_bundled_plugin_ships_its_folder_icon() {
    let config: serde_json::Value =
        serde_json::from_str(&read("tauri.conf.json")).expect("valid Tauri config");
    let resources = config["bundle"]["resources"]
        .as_object()
        .expect("resource map");
    for plugin in ["clock", "systeminfo", "media", "weather", "crypto", "todos"] {
        let source = format!("../../plugins/{plugin}/icon.png");
        assert_eq!(resources[&source], format!("plugins/{plugin}/icon.png"));
        assert!(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(source)
                .is_file()
        );
    }
}

#[test]
fn provider_identity_is_installed_before_the_manual_bar_window() {
    let config: serde_json::Value =
        serde_json::from_str(&read("tauri.conf.json")).expect("valid Tauri config");
    assert_eq!(config["identifier"], "dev.smabar.desktop");
    assert_eq!(config["app"]["enableGTKAppId"], true);
    let bar = config["app"]["windows"]
        .as_array()
        .expect("app.windows must be an array")
        .iter()
        .find(|window| window["label"] == "bar")
        .expect("bar window config");
    assert_eq!(bar["create"], false);
    assert_eq!(
        config["bundle"]["windows"]["minimumWebview2Version"],
        "122.0.2365.46"
    );

    let main = read("src/main.rs");
    let state = main
        .find("app.manage(commands::AppState::new(")
        .expect("state");
    let create = main
        .find("surface_manager.create_bar(app.handle())")
        .expect("manual bar creation");
    assert!(
        state < create,
        "IPC state must exist before the webview loads"
    );

    let linux_config: serde_json::Value =
        serde_json::from_str(&read("tauri.linux.conf.json")).expect("Linux Tauri config");
    assert_eq!(
        linux_config["bundle"]["resources"]["web-extensions/*.so"],
        "web-extensions/"
    );
    let linux = read("src/platform/linux/provider_identity.c");
    assert!(
        linux.contains("#define APP_IDENTITY \"https://dev.smabar.desktop/\"")
            && linux.contains("wrapper_origin != NULL")
            && linux.contains("g_strcmp0(referer, wrapper_origin) == 0")
            && linux.contains("\"user-message-received\"")
            && linux.contains("soup_message_headers_replace(headers, \"Referer\", APP_IDENTITY)")
    );
    let linux_window = read("src/platform/linux/window.rs");
    let linux_host = read("src/platform/linux/provider_identity.rs");
    assert!(
        linux_window.contains("provider_identity::send")
            && linux_host.contains("send_message_to_all_extensions")
            && linux_host.contains("set_web_extensions_initialization_user_data")
            && linux_host.contains("recv_timeout")
            && !linux_host.contains(".try_recv()")
            && linux.contains("webkit_web_extension_initialize_with_user_data")
    );
    let windows = read("src/platform/windows/provider_identity.rs");
    assert!(
        windows.contains("AddWebResourceRequestedFilterWithRequestSourceKinds")
            && windows.contains("COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT")
            && windows.contains("headers.SetHeader(&name, &HSTRING::from(APP_IDENTITY))")
            && windows.contains("recv_timeout")
            && !windows.contains(".try_recv()")
    );
}

#[test]
fn app_exit_gracefully_stops_plugin_processes() {
    let main = read("src/main.rs");
    assert!(
        main.contains("app.state::<commands::AppState>().shutdown_plugins()"),
        "RunEvent::Exit must stop plugin child processes before Tauri exits directly"
    );
}

#[test]
fn app_update_stops_plugins_before_the_installer_takes_over() {
    let update = read("src/commands/update.rs");
    let shutdown = update
        .find("shutdown_plugins().await")
        .expect("install_update must stop plugin child processes");
    let install = update
        .find(".install(bytes)")
        .expect("install_update must hand the package to the updater plugin");
    assert!(
        shutdown < install,
        "the updater exits the process; plugins must be stopped before it runs"
    );
}

#[test]
fn cached_uv_sidecars_are_verified_before_the_early_exit() {
    let script = read("../../scripts/fetch-uv.sh");
    let cache_check = script
        .find("if [[ -f \"$DESTINATION\" && -f \"$STAMP\" ]]")
        .expect("fetch-uv must inspect an existing stamped sidecar");
    let download = script[cache_check..]
        .find("curl -fsSL")
        .map(|offset| cache_check + offset)
        .expect("fetch-uv must retain its download fallback");
    let early_exit = &script[cache_check..download];

    assert!(
        early_exit.contains("sha256_file \"$DESTINATION\"")
            && early_exit.contains("== \"$BINARY_SHA256\""),
        "a matching version stamp must not bypass the pinned binary checksum"
    );
    assert!(
        script[download..].contains("sha256_file \"$SOURCE\"")
            && script[download..].contains("!= \"$BINARY_SHA256\""),
        "the extracted sidecar must match the same in-repo binary pin"
    );
}

#[test]
fn linux_packages_keep_uv_private_and_windows_keeps_its_sidecar() {
    use tauri::utils::config::parse::read_from;
    use tauri::utils::platform::Target;

    let (linux, _) = read_from(Target::Linux, &crate_dir()).expect("Linux Tauri config");
    assert_eq!(linux["bundle"]["externalBin"], serde_json::json!([]));
    for format in ["deb", "rpm"] {
        let files = linux["bundle"]["linux"][format]["files"]
            .as_object()
            .expect("package files");
        assert_eq!(
            files["/usr/lib/smabar/tools/uv"],
            "binaries/uv-x86_64-unknown-linux-gnu"
        );
        assert!(!files.contains_key("/usr/bin/uv"));
        assert!(files.contains_key("/usr/share/metainfo/dev.smabar.desktop.metainfo.xml"));
    }

    let (windows, _) = read_from(Target::Windows, &crate_dir()).expect("Windows Tauri config");
    assert_eq!(
        windows["bundle"]["externalBin"],
        serde_json::json!(["binaries/uv"])
    );
}

#[test]
fn native_shortcut_waits_never_run_on_the_window_thread() {
    let shortcuts = read("src/commands/shortcuts.rs");
    for command in [
        "list_apps",
        "get_app_icon",
        "get_shortcuts",
        "pin_shortcut",
        "launch_shortcut",
    ] {
        assert!(
            shortcuts.contains(&format!("#[tauri::command(async)]\npub fn {command}")),
            "{command} can wait on native shortcut work and must stay off the window thread"
        );
    }

    let commands = read("src/commands/mod.rs");
    assert!(
        commands.contains("#[tauri::command(async)]\npub fn open_url"),
        "open_url can wait on the native opener and must stay off the window thread"
    );
}
