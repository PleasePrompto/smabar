//! Minimal Linux WebKit render selection.
//!
//! Hybrid systems with the proprietary NVIDIA driver pin WebKit to the first
//! non-NVIDIA render node: that driver flashes stale WebKit buffers on fast
//! flyout switches (WebKit 262607 WONTFIX). Every other GPU set uses the
//! desktop's native WebKit renderer. Virtio disables GTK's GL presentation,
//! whose fences can block the UI thread during Wayland surface movement.
//! Mesa software is the fallback when there is no DRM render node.
//!
//! The same pre-thread environment turns JavaScriptCore's JIT off: the shell
//! runs little JavaScript, and interpreting it costs the web process less
//! memory than JIT code plus its executable pool (ADR 0018).

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use serde::Serialize;

use crate::config::RenderingMode;

pub const DEVICE_VAR: &str = "WEBKIT_WEB_RENDER_DEVICE_FILE";
pub const MESA_VENDOR_FILE: &str = "/usr/share/glvnd/egl_vendor.d/50_mesa.json";
pub const NVIDIA_DRIVER: &str = "nvidia";
pub const GTK_GL_VAR: &str = "GDK_GL";
pub const JSC_JIT_VAR: &str = "JSC_useJIT";

const SOFTWARE_VARS: [&str; 3] = [
    "LIBGL_ALWAYS_SOFTWARE",
    "__EGL_VENDOR_LIBRARY_FILENAMES",
    "WEBKIT_SKIA_ENABLE_CPU_RENDERING",
];
pub const RENDER_VARS: [&str; 5] = [
    DEVICE_VAR,
    SOFTWARE_VARS[0],
    SOFTWARE_VARS[1],
    SOFTWARE_VARS[2],
    GTK_GL_VAR,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderNode {
    pub device: PathBuf,
    pub driver: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Applied {
    Native,
    /// WebKit pinned to the first non-NVIDIA render node (hybrid graphics).
    Pinned,
    Software,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPlan {
    pub requested: RenderingMode,
    pub applied: Applied,
    pub nvidia_detected: bool,
    pub env: Vec<(&'static str, String)>,
    pub note: Option<String>,
}

pub fn plan(
    mode: RenderingMode,
    nodes: &[RenderNode],
    mesa_available: bool,
    preset: &[&str],
) -> RenderPlan {
    let nvidia_detected = nodes.iter().any(|node| node.driver == NVIDIA_DRIVER);
    let native = |note| RenderPlan {
        requested: mode,
        applied: Applied::Native,
        nvidia_detected,
        env: Vec::new(),
        note,
    };

    if !preset.is_empty() {
        return RenderPlan {
            requested: mode,
            applied: if preset.iter().any(|name| SOFTWARE_VARS.contains(name)) {
                Applied::Software
            } else {
                Applied::Native
            },
            nvidia_detected,
            env: Vec::new(),
            note: Some(format!(
                "{} already set by the environment; smabar leaves rendering alone",
                preset.join(", ")
            )),
        };
    }
    let software = || {
        if !mesa_available {
            return native(Some(format!(
                "{MESA_VENDOR_FILE} is missing; install the Mesa EGL package \
                 (libegl-mesa0 / mesa-libEGL / mesa) or select native rendering"
            )));
        }
        RenderPlan {
            requested: mode,
            applied: Applied::Software,
            nvidia_detected,
            env: vec![
                (SOFTWARE_VARS[0], "1".to_string()),
                (SOFTWARE_VARS[1], MESA_VENDOR_FILE.to_string()),
                (SOFTWARE_VARS[2], "1".to_string()),
            ],
            note: None,
        }
    };

    match mode {
        RenderingMode::Native => native(None),
        RenderingMode::Software => software(),
        RenderingMode::Auto if nodes.is_empty() => software(),
        RenderingMode::Auto
            if nodes.iter().all(|node| {
                matches!(node.driver.as_str(), "virtio-pci" | "virtio_gpu")
            }) => RenderPlan {
                requested: mode,
                applied: Applied::Software,
                nvidia_detected,
                env: vec![(GTK_GL_VAR, "disable".to_string())],
                note: Some("Virtio GPU: GTK GL presentation disabled to keep native window movement responsive".to_string()),
            },
        // Hybrid graphics: the proprietary NVIDIA driver flashes stale WebKit
        // buffers on fast flyout switches (WebKit 262607 WONTFIX), so the web
        // process is pinned to the first other node — `nodes` arrives sorted
        // by device path. NVIDIA-only and non-NVIDIA systems stay native.
        RenderingMode::Auto => match nodes.iter().find(|node| node.driver != NVIDIA_DRIVER) {
            Some(node) if nvidia_detected => RenderPlan {
                requested: mode,
                applied: Applied::Pinned,
                nvidia_detected,
                env: vec![(DEVICE_VAR, node.device.display().to_string())],
                note: None,
            },
            _ => native(None),
        },
    }
}

/// JavaScriptCore reads `JSC_<option>` from the web process environment. A
/// user who set any `JSC_*` variable keeps full control.
pub fn jsc_env(preset: bool) -> Vec<(&'static str, String)> {
    if preset {
        Vec::new()
    } else {
        vec![(JSC_JIT_VAR, "false".to_string())]
    }
}

static APPLIED_ENV: OnceLock<Vec<&'static str>> = OnceLock::new();

impl RenderPlan {
    /// # Safety
    ///
    /// Must run before any process thread exists.
    pub unsafe fn apply(&self) {
        for (name, value) in &self.env {
            // SAFETY: upheld by the caller before logging and GTK start.
            unsafe { std::env::set_var(name, value) };
        }
        let _ = APPLIED_ENV.set(self.env.iter().map(|(name, _)| *name).collect());
    }
}

pub fn scrub_child_env(command: &mut Command) {
    for name in APPLIED_ENV.get().map_or(&[][..], Vec::as_slice) {
        command.env_remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(device: &str, driver: &str) -> RenderNode {
        RenderNode {
            device: PathBuf::from(device),
            driver: driver.to_string(),
        }
    }

    #[test]
    fn single_gpu_systems_use_native_rendering() {
        for nodes in [
            vec![node("/dev/dri/renderD128", NVIDIA_DRIVER)],
            vec![node("/dev/dri/renderD128", "amdgpu")],
        ] {
            let decision = plan(RenderingMode::Auto, &nodes, true, &[]);
            assert_eq!(decision.applied, Applied::Native);
            assert!(decision.env.is_empty());
        }
    }

    #[test]
    fn virtio_avoids_gtk_gl_fences_unless_rendering_is_explicit() {
        for driver in ["virtio-pci", "virtio_gpu"] {
            let nodes = [node("/dev/dri/renderD128", driver)];
            let decision = plan(RenderingMode::Auto, &nodes, true, &[]);
            assert_eq!(decision.applied, Applied::Software);
            assert_eq!(decision.env, vec![(GTK_GL_VAR, "disable".to_string())]);
            assert!(
                plan(RenderingMode::Native, &nodes, true, &[])
                    .env
                    .is_empty()
            );
            assert!(
                plan(RenderingMode::Auto, &nodes, true, &[GTK_GL_VAR])
                    .env
                    .is_empty()
            );
        }
        let mixed = [
            node("/dev/dri/renderD128", "virtio-pci"),
            node("/dev/dri/renderD129", "amdgpu"),
        ];
        assert!(plan(RenderingMode::Auto, &mixed, true, &[]).env.is_empty());
    }

    #[test]
    fn hybrid_nvidia_pins_webkit_to_the_first_other_node() {
        let decision = plan(
            RenderingMode::Auto,
            &[
                node("/dev/dri/renderD128", "amdgpu"),
                node("/dev/dri/renderD129", NVIDIA_DRIVER),
            ],
            true,
            &[],
        );
        assert_eq!(decision.applied, Applied::Pinned);
        assert_eq!(
            decision.env,
            vec![(DEVICE_VAR, "/dev/dri/renderD128".to_string())]
        );
        assert!(decision.nvidia_detected);
        assert!(decision.note.is_none());

        // The first NON-NVIDIA node wins, not the first node overall.
        let flipped = plan(
            RenderingMode::Auto,
            &[
                node("/dev/dri/renderD128", NVIDIA_DRIVER),
                node("/dev/dri/renderD129", "i915"),
            ],
            true,
            &[],
        );
        assert_eq!(flipped.applied, Applied::Pinned);
        assert_eq!(
            flipped.env,
            vec![(DEVICE_VAR, "/dev/dri/renderD129".to_string())]
        );
    }

    #[test]
    fn nouveau_counts_as_non_nvidia() {
        let decision = plan(
            RenderingMode::Auto,
            &[
                node("/dev/dri/renderD128", "nouveau"),
                node("/dev/dri/renderD129", NVIDIA_DRIVER),
            ],
            true,
            &[],
        );
        assert_eq!(decision.applied, Applied::Pinned);
        assert_eq!(
            decision.env,
            vec![(DEVICE_VAR, "/dev/dri/renderD128".to_string())]
        );
    }

    #[test]
    fn no_gpu_uses_mesa_software_rendering() {
        let decision = plan(RenderingMode::Auto, &[], true, &[]);
        assert_eq!(decision.applied, Applied::Software);
        assert_eq!(decision.env.len(), 3);
    }

    #[test]
    fn preserves_user_rendering_overrides() {
        // Hybrid pair: the preset must beat the pinning, which is what keeps
        // `scripts/dev-render.sh nvidia` usable as the explicit test tool.
        let decision = plan(
            RenderingMode::Auto,
            &[
                node("/dev/dri/renderD128", "amdgpu"),
                node("/dev/dri/renderD129", NVIDIA_DRIVER),
            ],
            true,
            &[DEVICE_VAR],
        );
        assert_eq!(decision.applied, Applied::Native);
        assert!(decision.env.is_empty());
        assert!(decision.note.expect("override note").contains(DEVICE_VAR));
    }

    #[test]
    fn reports_a_software_environment_override_as_software() {
        let decision = plan(
            RenderingMode::Auto,
            &[node("/dev/dri/renderD128", NVIDIA_DRIVER)],
            true,
            &[SOFTWARE_VARS[0]],
        );
        assert_eq!(decision.applied, Applied::Software);
        assert!(decision.env.is_empty());
    }

    #[test]
    fn missing_mesa_is_actionable() {
        let decision = plan(RenderingMode::Auto, &[], false, &[]);
        assert_eq!(decision.applied, Applied::Native);
        assert!(decision.note.expect("Mesa note").contains("libegl-mesa0"));
    }

    #[test]
    fn explicit_modes_override_automatic_gpu_detection() {
        let gpu = [node("/dev/dri/renderD128", "amdgpu")];
        let hybrid = [
            node("/dev/dri/renderD128", "amdgpu"),
            node("/dev/dri/renderD129", NVIDIA_DRIVER),
        ];
        assert_eq!(
            plan(RenderingMode::Native, &[], true, &[]).applied,
            Applied::Native
        );
        assert_eq!(
            plan(RenderingMode::Software, &gpu, true, &[]).applied,
            Applied::Software
        );
        let explicit = plan(RenderingMode::Native, &hybrid, true, &[]);
        assert_eq!(explicit.applied, Applied::Native);
        assert!(explicit.env.is_empty());
    }

    #[test]
    fn jit_stays_off_unless_the_user_set_a_jsc_option() {
        assert_eq!(jsc_env(false), vec![(JSC_JIT_VAR, "false".to_string())]);
        assert!(jsc_env(true).is_empty());
    }

    #[test]
    fn scrub_is_empty_before_a_plan_is_applied() {
        let mut command = Command::new("true");
        scrub_child_env(&mut command);
        assert_eq!(command.get_envs().count(), 0);
    }
}
