use serde_json::Value;

use crate::params::*;

pub(crate) fn handle_desktop_shell_window_close(params: &Value) -> Result<(), String> {
    let action = param_string(params, &["action"])?;
    match action.as_str() {
        "minimize" | "quit" => {}
        _ => return Err(format!("invalid close action: {action}")),
    };
    let remember = param_optional_bool(params, &["remember"])?.unwrap_or(false);
    if !remember {
        return Ok(());
    }

    cockpit_core::modules::config::save_close_behavior_for_window_action(&action)
}

pub(crate) fn set_floating_card_always_on_top(params: &Value) -> Result<(), String> {
    let always_on_top = param_bool(params, &["alwaysOnTop", "always_on_top"])?;
    cockpit_core::modules::config::set_floating_card_always_on_top(always_on_top)
}

pub(crate) fn set_floating_card_confirm_on_close(params: &Value) -> Result<(), String> {
    let confirm_on_close = param_bool(params, &["confirmOnClose", "confirm_on_close"])?;
    cockpit_core::modules::config::set_floating_card_confirm_on_close(confirm_on_close)
}

pub(crate) fn save_floating_card_position(params: &Value) -> Result<(), String> {
    let x = param_i32(params, &["x"])?;
    let y = param_i32(params, &["y"])?;
    cockpit_core::modules::config::save_floating_card_position(x, y)
}

pub(crate) fn save_tray_platform_layout(
    params: &Value,
) -> Result<cockpit_core::modules::tray_layout::TrayLayoutConfig, String> {
    let sort_mode = param_string(params, &["sortMode", "sort_mode"])?;
    let ordered_platform_ids =
        param_string_vec(params, &["orderedPlatformIds", "ordered_platform_ids"])?;
    let tray_platform_ids = param_string_vec(params, &["trayPlatformIds", "tray_platform_ids"])?;
    let ordered_entry_ids =
        param_optional_string_vec(params, &["orderedEntryIds", "ordered_entry_ids"])?;
    let platform_groups = param_optional_tray_layout_groups(params)?;
    cockpit_core::modules::tray_layout::save_tray_layout(
        sort_mode,
        ordered_platform_ids,
        tray_platform_ids,
        ordered_entry_ids,
        platform_groups,
    )
}

pub(crate) fn detect_app_path(params: &Value) -> Result<Option<String>, String> {
    let app = param_string(params, &["app"])?;
    let force = param_optional_bool(params, &["force"])?.unwrap_or(false);
    cockpit_core::modules::system_host::detect_app_path(&app, force)
}
