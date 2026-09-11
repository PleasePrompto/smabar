//! Native Windows window glue: regional hit-testing and AppBar reservation.

pub mod capture;
pub mod input_shape;
pub mod runtime;
pub mod strut;
pub mod window;

mod appbar;
mod hit_test;
pub mod monitor;
mod native;
mod placement;
mod provider_identity;
mod proxy;
mod taskbar_order;

pub fn shutdown() -> anyhow::Result<()> {
    native::shutdown()
}
