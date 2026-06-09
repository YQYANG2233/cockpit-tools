use tauri::AppHandle;

#[tauri::command]
pub async fn get_provider_current_account_id(
    app: AppHandle,
    platform: String,
) -> Result<Option<String>, String> {
    let current_account_id =
        cockpit_core::modules::provider_current_state::resolve_provider_current_account_id(
            platform.trim(),
        )?;
    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(current_account_id)
}
