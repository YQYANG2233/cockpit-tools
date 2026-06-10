use serde_json::Value;

use crate::params::*;

pub(crate) fn data_transfer_apply_user_config(params: &Value) -> Result<bool, String> {
    let payload = params
        .as_object()
        .and_then(|params| params.get("config"))
        .cloned()
        .unwrap_or_else(|| params.clone());
    let imported_config: cockpit_core::modules::config::UserConfig =
        serde_json::from_value(payload).map_err(|err| format!("invalid user config: {err}"))?;
    let current = cockpit_core::modules::config::get_user_config();
    let current_app_auto_launch_enabled = current.app_auto_launch_enabled;

    let applied = cockpit_core::modules::data_transfer::apply_user_config(
        current,
        imported_config,
        current_app_auto_launch_enabled,
    )?;
    Ok(applied.needs_restart)
}

pub(crate) fn data_transfer_get_instance_store(
    params: &Value,
) -> Result<cockpit_core::models::InstanceStore, String> {
    let platform = param_string(params, &["platform"])?;
    cockpit_core::modules::data_transfer::load_instance_store_by_platform(platform.trim())
}

pub(crate) fn data_transfer_replace_instance_store(params: &Value) -> Result<(), String> {
    let platform = param_string(params, &["platform"])?;
    let store_value = params
        .as_object()
        .and_then(|params| params.get("store"))
        .cloned()
        .ok_or_else(|| "missing instance store".to_string())?;
    let store: cockpit_core::models::InstanceStore = serde_json::from_value(store_value)
        .map_err(|err| format!("invalid instance store: {err}"))?;
    cockpit_core::modules::data_transfer::replace_instance_store(platform.trim(), &store)
}

pub(crate) fn save_text_file(params: &Value) -> Result<(), String> {
    let path = param_string(params, &["path"])?;
    let content = param_string_or_empty(params, &["content"])?;
    cockpit_core::modules::system_host::save_text_file(path, &content)
}

pub(crate) fn read_text_file(params: &Value) -> Result<String, String> {
    let path = param_string(params, &["path"])?;
    cockpit_core::modules::system_host::read_text_file(path)
}

pub(crate) fn open_system_path(params: &Value) -> Result<(), String> {
    let path = param_string(params, &["path"])?;
    cockpit_core::modules::system_host::open_existing_path(path)
}

pub(crate) fn open_data_folder() -> Result<(), String> {
    cockpit_core::modules::system_host::open_data_folder()
}

pub(crate) fn open_folder(params: &Value) -> Result<(), String> {
    let path = param_string(params, &["path"])?;
    cockpit_core::modules::system_host::open_folder(path)
}

pub(crate) fn open_path_in_system(path: impl AsRef<std::path::Path>) -> Result<(), String> {
    cockpit_core::modules::system_host::open_path_in_system(path)
}
