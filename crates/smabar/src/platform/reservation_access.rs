//! Accessibility permission is requested only from the settings surface.

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReservationAccess {
    Ready,
    PermissionRequired,
}

fn status(request: bool) -> ReservationAccess {
    #[cfg(target_os = "macos")]
    let granted = super::macos::strut::permission(request);
    #[cfg(not(target_os = "macos"))]
    let granted = {
        let _ = request;
        true
    };
    if granted {
        ReservationAccess::Ready
    } else {
        ReservationAccess::PermissionRequired
    }
}

#[tauri::command]
pub fn get_reservation_status() -> ReservationAccess {
    status(false)
}

#[tauri::command]
pub fn request_reservation_access() -> ReservationAccess {
    status(true)
}
