//! Native media-session access. Providers and plugins only see neutral data
//! and actions; MPRIS and the Windows GlobalSystemMediaTransportControls stay
//! inside this module.

#[cfg(any(target_os = "linux", windows, test))]
mod common;
#[cfg(not(any(target_os = "linux", windows)))]
mod fallback;
// Pure GSMTC mapping; compiled under test on every host so its tests run on
// Linux, where the WinRT adapter itself cannot compile.
#[cfg(any(windows, test))]
mod gsmtc;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod winrt;

#[cfg(not(any(target_os = "linux", windows)))]
use fallback as imp;
#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(windows)]
use windows as imp;

pub(crate) use imp::{MEDIA_SUPPORTED, MediaError, MediaSource};
