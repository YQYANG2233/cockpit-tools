use serde_json::Value;

use crate::params::*;
use crate::rpc_types::block_on;
use crate::WebdavSyncSettings;

pub(crate) fn get_webdav_sync_settings() -> WebdavSyncSettings {
    cockpit_core::modules::auto_backup::get_webdav_sync_settings()
}

pub(crate) fn save_webdav_sync_settings(params: &Value) -> Result<WebdavSyncSettings, String> {
    let enabled = param_bool(params, &["enabled"])?;
    let url = param_string(params, &["url"])?;
    let username = param_string_or_empty(params, &["username"])?;
    let password = param_optional_string(params, &["password"])?;
    let clear_password = param_optional_bool(params, &["clearPassword", "clear_password"])?;
    let remote_dir = param_string(params, &["remoteDir", "remote_dir"])?;
    let retention_days = param_optional_i64(params, &["retentionDays", "retention_days"])?
        .and_then(|value| i32::try_from(value).ok());
    cockpit_core::modules::auto_backup::save_webdav_sync_settings(
        enabled,
        &url,
        &username,
        password,
        clear_password,
        &remote_dir,
        retention_days,
    )
}

pub(crate) fn test_webdav_sync_connection(
    params: &Value,
) -> Result<cockpit_core::modules::webdav_sync::WebdavTestResult, String> {
    let url = param_string(params, &["url"])?;
    let username = param_string_or_empty(params, &["username"])?;
    let password = param_optional_string(params, &["password"])?;
    let clear_password = param_optional_bool(params, &["clearPassword", "clear_password"])?;
    let remote_dir = param_string(params, &["remoteDir", "remote_dir"])?;
    block_on(
        cockpit_core::modules::auto_backup::test_webdav_sync_connection(
            &url,
            &username,
            password,
            clear_password,
            &remote_dir,
        ),
    )
}

pub(crate) fn upload_auto_backup_to_webdav(
    params: &Value,
) -> Result<cockpit_core::modules::webdav_sync::WebdavUploadResult, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    block_on(cockpit_core::modules::auto_backup::upload_auto_backup_to_webdav(&file_name))
}

pub(crate) fn list_webdav_backup_files(
) -> Result<Vec<cockpit_core::modules::webdav_sync::WebdavBackupFileEntry>, String> {
    block_on(cockpit_core::modules::auto_backup::list_webdav_backup_files())
}

pub(crate) fn read_webdav_backup_file(params: &Value) -> Result<String, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    block_on(cockpit_core::modules::auto_backup::read_webdav_backup_file(
        &file_name,
    ))
}

pub(crate) fn delete_webdav_backup_file(params: &Value) -> Result<(), String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    block_on(cockpit_core::modules::auto_backup::delete_webdav_backup_file(&file_name))
}
