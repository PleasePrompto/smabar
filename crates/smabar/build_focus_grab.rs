//! Build the small GTK/Wayland bridge against generated upstream protocol bindings.
use std::{path::PathBuf, process::Command};

pub fn build() -> Result<(), String> {
    let out = PathBuf::from(super::required_env("OUT_DIR")?);
    let source = "src/platform/linux/focus_grab.c";
    let protocol = "src/platform/linux/hyprland-focus-grab-v1.xml";
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-changed={protocol}");
    for (mode, filename) in [
        ("client-header", "hyprland-focus-grab-v1.h"),
        ("private-code", "hyprland-focus-grab-v1.c"),
    ] {
        run(Command::new("wayland-scanner")
            .arg(mode)
            .arg(protocol)
            .arg(out.join(filename)))?;
    }
    let flags = Command::new("pkg-config")
        .args(["--cflags", "gtk+-wayland-3.0", "wayland-client"])
        .output()
        .map_err(|e| e.to_string())?;
    if !flags.status.success() {
        return Err(String::from_utf8_lossy(&flags.stderr).into_owned());
    }
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let generated = out.join("hyprland-focus-grab-v1.c");
    let mut objects = Vec::new();
    for (index, input) in [PathBuf::from(source), generated].into_iter().enumerate() {
        let object = out.join(format!("focus-grab-{index}.o"));
        run(Command::new(&compiler)
            .args(["-c", "-fPIC", "-Os", "-Wall", "-Wextra", "-Werror"])
            .args(String::from_utf8_lossy(&flags.stdout).split_whitespace())
            .arg("-I")
            .arg(&out)
            .arg(input)
            .arg("-o")
            .arg(&object))?;
        objects.push(object);
    }
    run(Command::new("ar")
        .arg("crs")
        .arg(out.join("libsmabar-focus-grab.a"))
        .args(objects))?;
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=smabar-focus-grab");
    println!("cargo:rustc-link-lib=wayland-client");
    Ok(())
}

fn run(command: &mut Command) -> Result<(), String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}
