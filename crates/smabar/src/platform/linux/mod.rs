//! Linux window glue: X11 struts and input shapes, Wayland layer-shell.

pub mod capture;
mod diagnostics;
mod focus_grab;
pub mod input_shape;
mod layer;
pub mod monitor;
mod provider_identity;
pub mod render;
mod reservation;
pub mod scale;
pub mod strut;
mod surface_monitor;
pub(crate) mod transition;
pub mod window;
