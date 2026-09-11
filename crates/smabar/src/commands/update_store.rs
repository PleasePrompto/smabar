//! Same IPC entrypoints as direct installs, with no self-update implementation.

#[tauri::command]
pub async fn check_update() -> Result<Option<()>, String> {
    Ok(None)
}

#[tauri::command]
pub async fn install_update(expected_version: String) -> Result<(), String> {
    tracing::warn!(version = %expected_version, "self-update refused; use Microsoft Store > Library to update smabar");
    Err("Updates for this installation are delivered by Microsoft Store. Open Store > Library to check for updates.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn store_never_offers_or_installs_an_application_update() {
        assert_eq!(check_update().await, Ok(None));
        assert!(install_update("1.0.1".into()).await.is_err());
    }
}
