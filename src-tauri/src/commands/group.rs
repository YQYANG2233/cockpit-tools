use cockpit_core::modules::group_settings::{self, DisplayGroup, GroupSettings};
use std::collections::HashMap;

fn refresh_tray_menu(app: &tauri::AppHandle) {
    if let Err(err) = crate::modules::tray::update_tray_menu(app) {
        crate::modules::logger::log_warn(&format!(
            "[GroupSettings] failed to update tray menu: {err}"
        ));
    }
}

#[tauri::command]
pub fn get_group_settings() -> Result<GroupSettings, String> {
    Ok(group_settings::load_group_settings())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn save_group_settings(
    app: tauri::AppHandle,
    groupMappings: HashMap<String, String>,
    groupNames: HashMap<String, String>,
    groupOrder: Vec<String>,
) -> Result<(), String> {
    group_settings::replace_group_settings(groupMappings, groupNames, groupOrder)?;
    refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn set_model_group(
    app: tauri::AppHandle,
    modelId: String,
    groupId: String,
) -> Result<(), String> {
    group_settings::mutate_group_settings(|settings| settings.set_model_group(&modelId, &groupId))?;
    refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn remove_model_group(app: tauri::AppHandle, modelId: String) -> Result<(), String> {
    group_settings::mutate_group_settings(|settings| settings.remove_model_group(&modelId))?;
    refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn set_group_name(app: tauri::AppHandle, groupId: String, name: String) -> Result<(), String> {
    group_settings::mutate_group_settings(|settings| settings.set_group_name(&groupId, &name))?;
    refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn delete_group(app: tauri::AppHandle, groupId: String) -> Result<(), String> {
    group_settings::mutate_group_settings(|settings| settings.delete_group(&groupId))?;
    refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn update_group_order(app: tauri::AppHandle, order: Vec<String>) -> Result<(), String> {
    group_settings::mutate_group_settings(|settings| settings.set_group_order(order))?;
    refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn get_display_groups() -> Result<Vec<DisplayGroup>, String> {
    Ok(group_settings::get_display_groups())
}
