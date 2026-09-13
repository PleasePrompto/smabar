//! Linux DRM discovery for WebKit's automatic render selection, plus the
//! JavaScriptCore JIT switch for the web process.

use std::path::Path;

use smabar_core::config::RenderingMode;
use smabar_core::platform::render::{self, RenderNode, RenderPlan};

const DRM_CLASS_DIR: &str = "/sys/class/drm";
const DEVICE_DIR: &str = "/dev/dri";

pub fn prepare(mode: RenderingMode) -> RenderPlan {
    let nodes = render_nodes(Path::new(DRM_CLASS_DIR), Path::new(DEVICE_DIR));
    let preset: Vec<&str> = render::RENDER_VARS
        .iter()
        .copied()
        .filter(|name| std::env::var_os(name).is_some())
        .collect();
    let mut plan = render::plan(
        mode,
        &nodes,
        Path::new(render::MESA_VENDOR_FILE).is_file(),
        &preset,
    );
    plan.env.extend(render::jsc_env(jsc_preset()));
    // SAFETY: main calls this before logging, GTK, or any other thread starts.
    unsafe { plan.apply() };
    plan
}

fn jsc_preset() -> bool {
    std::env::vars_os().any(|(name, _)| name.to_string_lossy().starts_with("JSC_"))
}

fn render_nodes(class_dir: &Path, device_dir: &Path) -> Vec<RenderNode> {
    let Ok(entries) = std::fs::read_dir(class_dir) else {
        return Vec::new();
    };
    let mut nodes: Vec<RenderNode> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if !name.starts_with("renderD") {
                return None;
            }
            let uevent = std::fs::read_to_string(entry.path().join("device/uevent")).ok()?;
            Some(RenderNode {
                device: device_dir.join(name),
                driver: driver_from_uevent(&uevent)?,
            })
        })
        .collect();
    nodes.sort_by(|left, right| left.device.cmp(&right.device));
    nodes
}

fn driver_from_uevent(uevent: &str) -> Option<String> {
    uevent
        .lines()
        .find_map(|line| line.strip_prefix("DRIVER="))
        .map(|driver| driver.trim().to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn lists_render_nodes_in_device_order() {
        let sysfs = tempfile::tempdir().expect("tempdir");
        for (name, uevent) in [
            ("renderD129", "DRIVER=nvidia\n"),
            ("renderD128", "DRIVER=amdgpu\n"),
            ("card0", "DRIVER=amdgpu\n"),
            ("renderD130", "PCI_CLASS=30000\n"),
        ] {
            let device = sysfs.path().join(name).join("device");
            std::fs::create_dir_all(&device).expect("mkdir");
            std::fs::write(device.join("uevent"), uevent).expect("uevent");
        }

        assert_eq!(
            render_nodes(sysfs.path(), Path::new("/dev/dri")),
            vec![
                RenderNode {
                    device: PathBuf::from("/dev/dri/renderD128"),
                    driver: "amdgpu".to_string(),
                },
                RenderNode {
                    device: PathBuf::from("/dev/dri/renderD129"),
                    driver: "nvidia".to_string(),
                },
            ]
        );
    }
}
