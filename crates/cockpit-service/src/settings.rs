use serde_json::Value;

use crate::params::*;
use crate::rpc_types::to_value_result;

pub(crate) fn load_update_settings(
) -> Result<cockpit_core::modules::update_checker::UpdateSettings, String> {
    cockpit_core::modules::update_checker::load_update_settings()
}

pub(crate) fn save_update_settings(
    settings: &cockpit_core::modules::update_checker::UpdateSettings,
) -> Result<(), String> {
    cockpit_core::modules::update_checker::save_update_settings(settings)
}

pub(crate) fn update_last_check_time() -> Result<(), String> {
    cockpit_core::modules::update_checker::update_last_check_time()
}

pub(crate) fn should_check_updates() -> Result<bool, String> {
    let settings = load_update_settings()?;
    Ok(cockpit_core::modules::update_checker::should_check_for_updates(&settings))
}

pub(crate) fn check_version_jump(
) -> Result<Option<cockpit_core::modules::update_checker::VersionJumpInfo>, String> {
    cockpit_core::modules::update_checker::check_version_jump_for_version(&crate::app_version())
}

pub(crate) fn update_log(params: &Value) -> Result<(), String> {
    let level = param_string_or_empty(params, &["level"])?
        .trim()
        .to_lowercase();
    let message = param_string_or_empty(params, &["message"])?
        .trim()
        .to_string();
    if message.is_empty() {
        return Ok(());
    }

    let text = format!("[Updater] {message}");
    match level.as_str() {
        "error" => cockpit_core::modules::logger::log_error(&text),
        "warn" | "warning" => cockpit_core::modules::logger::log_warn(&text),
        _ => cockpit_core::modules::logger::log_info(&text),
    }
    Ok(())
}

pub(crate) fn save_pending_update_notes_values(
    version: String,
    release_notes: String,
    release_notes_zh: String,
) -> Result<(), String> {
    cockpit_core::modules::update_checker::save_pending_update_notes(
        version,
        release_notes,
        release_notes_zh,
    )
}

pub(crate) fn save_pending_update_notes(params: &Value) -> Result<(), String> {
    let version = param_string(params, &["version"])?;
    let release_notes = param_string_or_empty(params, &["releaseNotes", "release_notes"])?;
    let release_notes_zh = param_string_or_empty(params, &["releaseNotesZh", "release_notes_zh"])?;
    save_pending_update_notes_values(version, release_notes, release_notes_zh)
}

pub(crate) fn get_release_history(params: &Value) -> Result<Value, String> {
    let locale = param_optional_string(params, &["locale"])?;
    let limit =
        param_optional_u64(params, &["limit"])?.and_then(|value| usize::try_from(value).ok());
    to_value_result(cockpit_core::modules::update_checker::get_release_history(
        locale.as_deref(),
        limit,
    ))
}

pub(crate) fn get_network_config() -> Result<cockpit_core::modules::config::NetworkConfig, String> {
    Ok(cockpit_core::modules::config::get_network_config(None))
}

pub(crate) fn save_network_config(params: &Value) -> Result<bool, String> {
    cockpit_core::modules::config::save_network_config(
        cockpit_core::modules::config::NetworkConfigUpdate {
            ws_enabled: param_bool(params, &["wsEnabled", "ws_enabled"])?,
            ws_port: param_u16(params, &["wsPort", "ws_port"])?,
            report_enabled: param_optional_bool(params, &["reportEnabled", "report_enabled"])?,
            report_port: param_optional_u16(params, &["reportPort", "report_port"])?,
            report_token: param_optional_string(params, &["reportToken", "report_token"])?,
            global_proxy_enabled: param_optional_bool(
                params,
                &["globalProxyEnabled", "global_proxy_enabled"],
            )?,
            global_proxy_url: param_optional_string(
                params,
                &["globalProxyUrl", "global_proxy_url"],
            )?,
            global_proxy_no_proxy: param_optional_string(
                params,
                &["globalProxyNoProxy", "global_proxy_no_proxy"],
            )?,
        },
    )
}

pub(crate) fn set_app_path(params: &Value) -> Result<(), String> {
    let app = param_string(params, &["app"])?;
    let path = param_string_or_empty(params, &["path"])?;
    cockpit_core::modules::config::set_app_path(&app, path)
}

pub(crate) fn set_codex_launch_on_switch(params: &Value) -> Result<(), String> {
    let enabled = param_bool(params, &["enabled"])?;
    cockpit_core::modules::config::set_codex_launch_on_switch(enabled)
}

pub(crate) fn set_codex_local_access_entry_visible(params: &Value) -> Result<(), String> {
    let enabled = param_bool(params, &["enabled"])?;
    cockpit_core::modules::config::set_codex_local_access_entry_visible(enabled)
}
