use serde_json::Value;

use crate::params::*;
use crate::{AutoBackupFileEntry, AutoBackupSettings};

fn auto_backup_retention_days(params: &Value) -> Result<i32, String> {
    let value = param_optional_i64(params, &["retentionDays", "retention_days"])?.unwrap_or(3);
    let value = i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX });
    Ok(cockpit_core::modules::config::sanitize_auto_backup_retention_days(value))
}

pub(crate) fn get_auto_backup_settings() -> Result<AutoBackupSettings, String> {
    cockpit_core::modules::auto_backup::get_auto_backup_settings()
}

pub(crate) fn save_auto_backup_settings(params: &Value) -> Result<AutoBackupSettings, String> {
    let enabled = param_bool(params, &["enabled"])?;
    let include_accounts = param_bool(params, &["includeAccounts", "include_accounts"])?;
    let include_config = param_bool(params, &["includeConfig", "include_config"])?;
    let retention_days = auto_backup_retention_days(params)?;
    cockpit_core::modules::auto_backup::save_auto_backup_settings(
        enabled,
        include_accounts,
        include_config,
        retention_days,
    )
}

pub(crate) fn update_auto_backup_last_run(
    params: &Value,
) -> Result<AutoBackupSettings, String> {
    let last_backup_at = param_optional_string(params, &["lastBackupAt", "last_backup_at"])?;
    cockpit_core::modules::auto_backup::update_auto_backup_last_run(last_backup_at)
}

pub(crate) fn write_auto_backup_file(params: &Value) -> Result<String, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    let content = param_string_or_empty(params, &["content"])?;
    cockpit_core::modules::auto_backup::write_auto_backup_file(&file_name, &content)
}

pub(crate) fn read_auto_backup_file(params: &Value) -> Result<String, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    cockpit_core::modules::auto_backup::read_auto_backup_file(&file_name)
}

pub(crate) fn copy_auto_backup_file(params: &Value) -> Result<String, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    let target_path = param_string(params, &["targetPath", "target_path"])?;
    cockpit_core::modules::auto_backup::copy_auto_backup_file(&file_name, &target_path)
}

pub(crate) fn list_auto_backup_files() -> Result<Vec<AutoBackupFileEntry>, String> {
    cockpit_core::modules::auto_backup::list_auto_backup_files()
}

pub(crate) fn delete_auto_backup_file(params: &Value) -> Result<(), String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    cockpit_core::modules::auto_backup::delete_auto_backup_file(&file_name)
}

pub(crate) fn cleanup_auto_backup_files(params: &Value) -> Result<Vec<String>, String> {
    let retention_days = auto_backup_retention_days(params)?;
    cockpit_core::modules::auto_backup::cleanup_auto_backup_files(retention_days)
}

pub(crate) fn open_auto_backup_dir() -> Result<(), String> {
    cockpit_core::modules::auto_backup::open_auto_backup_dir()
}
