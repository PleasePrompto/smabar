//! Windows Shell AppBar calls and physical monitor geometry.

use anyhow::{Context, bail};
use smabar_core::platform::Rect;
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    UI::{
        Shell::{
            ABE_BOTTOM, ABE_TOP, ABM_ACTIVATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
            ABM_WINDOWPOSCHANGED, APPBARDATA, SHAppBarMessage,
        },
        WindowsAndMessaging::{GetClientRect, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos},
    },
};

use crate::platform::windows_geometry::{
    DockEdge, PhysicalRect, fit_appbar, physical_length, reservation_thickness,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct Reservation {
    pub edge: DockEdge,
    pub bar: Rect,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct AppliedReservation {
    edge: DockEdge,
    thickness: i32,
    dpi: u32,
    scale: f64,
    monitor: PhysicalRect,
    approved: PhysicalRect,
    work_area: PhysicalRect,
    logical_thickness: f64,
}

pub(super) fn register(hwnd: HWND, callback_message: u32) -> anyhow::Result<()> {
    let mut data = data(hwnd);
    data.uCallbackMessage = callback_message;
    // SAFETY: data is initialized for this live HWND and remains valid for the call.
    if unsafe { SHAppBarMessage(ABM_NEW, &mut data) } == 0 {
        bail!("Windows rejected ABM_NEW while registering the bar as an AppBar");
    }
    Ok(())
}

pub(super) fn remove(hwnd: HWND) -> anyhow::Result<()> {
    let mut data = data(hwnd);
    // SAFETY: data is initialized for this HWND and remains valid for the call.
    if unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) } == 0 {
        bail!("Windows rejected ABM_REMOVE while releasing the AppBar reservation");
    }
    Ok(())
}

pub(super) fn activate(hwnd: HWND) {
    let mut data = data(hwnd);
    // SAFETY: only cbSize and hWnd are consumed for ABM_ACTIVATE.
    unsafe { SHAppBarMessage(ABM_ACTIVATE, &mut data) };
}

pub(super) fn window_pos_changed(hwnd: HWND) {
    let mut data = data(hwnd);
    // SAFETY: only cbSize and hWnd are consumed for ABM_WINDOWPOSCHANGED.
    unsafe { SHAppBarMessage(ABM_WINDOWPOSCHANGED, &mut data) };
}

/// Physical thickness the reservation asks for, or `None` while the shell's
/// rect does not reach into the host window yet (bootstrap frame).
pub(super) fn thickness(host: HWND, reservation: Reservation) -> anyhow::Result<Option<i32>> {
    let client_height = client_height(host)?;
    let (_, scale) = super::native::window_scale(host)?;
    Ok(reservation_thickness(
        reservation.edge,
        reservation.bar,
        client_height,
        scale,
    ))
}

pub(super) fn reposition(
    appbar: HWND,
    host: HWND,
    reservation: Reservation,
    logical_thickness: f64,
) -> anyhow::Result<AppliedReservation> {
    let client_height = client_height(host)?;
    let (_, scale) = super::native::window_scale(host)?;
    let thickness = physical_length(logical_thickness, scale, client_height)
        .context("stored bar geometry produced an empty Windows AppBar reservation")?;
    position(appbar, host, reservation, thickness)
}

pub(super) fn position(
    appbar: HWND,
    host: HWND,
    reservation: Reservation,
    thickness: i32,
) -> anyhow::Result<AppliedReservation> {
    let monitor = monitor_rect(host)?;
    let (dpi, scale) = super::native::window_scale(host)?;

    let mut data = data(appbar);
    data.uEdge = edge_value(reservation.edge);
    data.rc = to_win_rect(monitor);
    // SAFETY: APPBARDATA is fully initialized and writable for the shell call.
    if unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut data) } == 0 {
        bail!("Windows rejected ABM_QUERYPOS for the bar reservation");
    }
    let approved = fit_appbar(reservation.edge, from_win_rect(data.rc), thickness);
    data.rc = to_win_rect(approved);
    // SAFETY: APPBARDATA is fully initialized and writable for the shell call.
    if unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) } == 0 {
        bail!("Windows rejected ABM_SETPOS for the bar reservation");
    }
    let approved = from_win_rect(data.rc);
    move_appbar(appbar, approved)?;
    let work_area = from_win_rect(monitor_info(host)?.rcWork);

    let applied = AppliedReservation {
        edge: reservation.edge,
        thickness,
        dpi,
        scale,
        monitor,
        approved,
        work_area,
        logical_thickness: f64::from(thickness) / scale,
    };
    Ok(applied)
}

fn move_appbar(hwnd: HWND, area: PhysicalRect) -> anyhow::Result<()> {
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            area.left,
            area.top,
            area.right.saturating_sub(area.left),
            area.bottom.saturating_sub(area.top),
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    }
    .context("failed to move the Windows AppBar proxy to its approved bounds")
}

pub(super) fn logical_thickness(applied: AppliedReservation) -> f64 {
    applied.logical_thickness
}

pub(super) fn log(
    host: HWND,
    appbar: HWND,
    bar: Rect,
    applied: AppliedReservation,
    message: &'static str,
) {
    tracing::info!(
        host_hwnd = super::native::raw_hwnd(host),
        appbar_hwnd = super::native::raw_hwnd(appbar),
        edge = ?applied.edge,
        logical_x = bar.x,
        logical_y = bar.y,
        logical_width = bar.w,
        logical_height = bar.h,
        reserved_thickness = applied.thickness,
        dpi = applied.dpi,
        scale_factor = applied.scale,
        monitor_left = applied.monitor.left,
        monitor_top = applied.monitor.top,
        monitor_right = applied.monitor.right,
        monitor_bottom = applied.monitor.bottom,
        reserved_left = applied.approved.left,
        reserved_top = applied.approved.top,
        reserved_right = applied.approved.right,
        reserved_bottom = applied.approved.bottom,
        work_left = applied.work_area.left,
        work_top = applied.work_area.top,
        work_right = applied.work_area.right,
        work_bottom = applied.work_area.bottom,
        "{message}"
    );
}

fn data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        ..Default::default()
    }
}

fn edge_value(edge: DockEdge) -> u32 {
    match edge {
        DockEdge::Top => ABE_TOP,
        DockEdge::Bottom => ABE_BOTTOM,
    }
}

fn monitor_rect(hwnd: HWND) -> anyhow::Result<PhysicalRect> {
    Ok(from_win_rect(monitor_info(hwnd)?.rcMonitor))
}

fn monitor_info(hwnd: HWND) -> anyhow::Result<MONITORINFO> {
    // SAFETY: hwnd is the live bar window; the function does not retain it.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.0.is_null() {
        bail!("Windows could not resolve a monitor for the bar window");
    }
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: info points to initialized writable storage for the duration of the call.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(windows::core::Error::from_thread())
            .context("failed to query the Windows bar monitor bounds");
    }
    Ok(info)
}

fn client_height(hwnd: HWND) -> anyhow::Result<i32> {
    let mut rect = RECT::default();
    // SAFETY: rect is writable and hwnd is the live bar window.
    unsafe { GetClientRect(hwnd, &mut rect) }
        .context("failed to query the Windows bar client rectangle")?;
    Ok(rect.bottom.saturating_sub(rect.top))
}

fn to_win_rect(rect: PhysicalRect) -> RECT {
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

fn from_win_rect(rect: RECT) -> PhysicalRect {
    PhysicalRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}
