use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::modules::{account, atomic_write, config, system_host, webdav_sync};

const AUTO_BACKUP_DIR_NAME: &str = "backups";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBackupSettings {
    pub enabled: bool,
    pub include_accounts: bool,
    pub include_config: bool,
    pub retention_days: i32,
    pub last_backup_at: Option<String>,
    pub directory_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBackupFileEntry {
    pub file_name: String,
    pub path: String,
    pub file_kind: String,
    pub size_bytes: u64,
    pub modified_at_ms: Option<i64>,
    pub archive_file_name: Option<String>,
    pub archive_path: Option<String>,
    pub archive_size_bytes: Option<u64>,
    pub platforms: Vec<AutoBackupPlatformEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBackupPlatformEntry {
    pub platform: String,
    pub account_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebdavSyncSettings {
    pub enabled: bool,
    pub url: String,
    pub username: String,
    pub has_password: bool,
    pub remote_dir: String,
    pub last_upload_at: Option<String>,
    pub last_upload_file_name: Option<String>,
    pub last_download_at: Option<String>,
    pub last_download_file_name: Option<String>,
    pub retention_days: i32,
}

pub fn auto_backup_dir_path() -> Result<PathBuf, String> {
    Ok(account::get_data_dir()?.join(AUTO_BACKUP_DIR_NAME))
}

pub fn ensure_auto_backup_dir_path() -> Result<PathBuf, String> {
    let dir = auto_backup_dir_path()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|err| format!("create auto backup dir failed: {err}"))?;
    }
    Ok(dir)
}

pub fn build_auto_backup_settings(
    user_config: &config::UserConfig,
) -> Result<AutoBackupSettings, String> {
    let (include_accounts, include_config) = config::normalize_auto_backup_selection(
        user_config.auto_backup_include_accounts,
        user_config.auto_backup_include_config,
    );
    Ok(AutoBackupSettings {
        enabled: user_config.auto_backup_enabled,
        include_accounts,
        include_config,
        retention_days: config::sanitize_auto_backup_retention_days(
            user_config.auto_backup_retention_days,
        ),
        last_backup_at: user_config.auto_backup_last_backup_at.clone(),
        directory_path: auto_backup_dir_path()?.to_string_lossy().to_string(),
    })
}

pub fn get_auto_backup_settings() -> Result<AutoBackupSettings, String> {
    let user_config = config::get_user_config();
    build_auto_backup_settings(&user_config)
}

pub fn save_auto_backup_settings(
    enabled: bool,
    include_accounts: bool,
    include_config: bool,
    retention_days: i32,
) -> Result<AutoBackupSettings, String> {
    let current = config::get_user_config();
    let (include_accounts, include_config) =
        config::normalize_auto_backup_selection(include_accounts, include_config);
    let next = config::UserConfig {
        auto_backup_enabled: enabled,
        auto_backup_include_accounts: include_accounts,
        auto_backup_include_config: include_config,
        auto_backup_retention_days: config::sanitize_auto_backup_retention_days(retention_days),
        ..current
    };
    config::save_user_config(&next)?;
    build_auto_backup_settings(&next)
}

pub fn update_auto_backup_last_run(
    last_backup_at: Option<String>,
) -> Result<AutoBackupSettings, String> {
    let current = config::get_user_config();
    let normalized_last_backup_at = last_backup_at.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let next = config::UserConfig {
        auto_backup_last_backup_at: normalized_last_backup_at,
        ..current
    };
    config::save_user_config(&next)?;
    build_auto_backup_settings(&next)
}

pub fn sanitize_auto_backup_file_name(file_name: &str) -> Result<String, String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return Err("backup file name cannot be empty".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("backup file name is invalid".to_string());
    }
    if !trimmed.ends_with(".json") && !trimmed.ends_with(".zip") {
        return Err("auto backup file must be JSON or ZIP".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn auto_backup_file_path(file_name: &str) -> Result<PathBuf, String> {
    let safe_name = sanitize_auto_backup_file_name(file_name)?;
    Ok(auto_backup_dir_path()?.join(safe_name))
}

pub fn auto_backup_archive_file_name(file_name: &str) -> Option<String> {
    file_name
        .strip_suffix(".json")
        .map(|stem| format!("{stem}.zip"))
}

pub fn auto_backup_json_file_name(file_name: &str) -> Option<String> {
    file_name
        .strip_suffix(".zip")
        .map(|stem| format!("{stem}.json"))
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn push_u16_le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32_le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn push_zip_entry(
    out: &mut Vec<u8>,
    central: &mut Vec<(String, u32, u32, u32)>,
    name: &str,
    content: &[u8],
) -> Result<(), String> {
    let name_bytes = name.as_bytes();
    let name_len =
        u16::try_from(name_bytes.len()).map_err(|_| format!("zip entry name too long: {name}"))?;
    let size = u32::try_from(content.len()).map_err(|_| format!("zip entry too large: {name}"))?;
    let offset = u32::try_from(out.len()).map_err(|_| "zip file too large".to_string())?;
    let crc = crc32(content);
    let dos_time = 0u16;
    let dos_date = 33u16;

    push_u32_le(out, 0x0403_4b50);
    push_u16_le(out, 20);
    push_u16_le(out, 0);
    push_u16_le(out, 0);
    push_u16_le(out, dos_time);
    push_u16_le(out, dos_date);
    push_u32_le(out, crc);
    push_u32_le(out, size);
    push_u32_le(out, size);
    push_u16_le(out, name_len);
    push_u16_le(out, 0);
    out.extend_from_slice(name_bytes);
    out.extend_from_slice(content);

    central.push((name.to_string(), crc, size, offset));
    Ok(())
}

fn build_stored_zip(entries: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
    if entries.is_empty() {
        return Err("zip entries cannot be empty".to_string());
    }

    let mut out = Vec::new();
    let mut central_entries = Vec::new();
    for (name, content) in entries {
        push_zip_entry(&mut out, &mut central_entries, &name, &content)?;
    }

    let central_offset = u32::try_from(out.len()).map_err(|_| "zip file too large".to_string())?;
    let dos_time = 0u16;
    let dos_date = 33u16;

    for (name, crc, size, offset) in &central_entries {
        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| format!("zip entry name too long: {name}"))?;
        push_u32_le(&mut out, 0x0201_4b50);
        push_u16_le(&mut out, 20);
        push_u16_le(&mut out, 20);
        push_u16_le(&mut out, 0);
        push_u16_le(&mut out, 0);
        push_u16_le(&mut out, dos_time);
        push_u16_le(&mut out, dos_date);
        push_u32_le(&mut out, *crc);
        push_u32_le(&mut out, *size);
        push_u32_le(&mut out, *size);
        push_u16_le(&mut out, name_len);
        push_u16_le(&mut out, 0);
        push_u16_le(&mut out, 0);
        push_u16_le(&mut out, 0);
        push_u16_le(&mut out, 0);
        push_u32_le(&mut out, 0);
        push_u32_le(&mut out, *offset);
        out.extend_from_slice(name_bytes);
    }

    let central_size = u32::try_from(out.len())
        .ok()
        .and_then(|len| len.checked_sub(central_offset))
        .ok_or_else(|| "zip central directory too large".to_string())?;
    let entry_count =
        u16::try_from(central_entries.len()).map_err(|_| "too many zip entries".to_string())?;

    push_u32_le(&mut out, 0x0605_4b50);
    push_u16_le(&mut out, 0);
    push_u16_le(&mut out, 0);
    push_u16_le(&mut out, entry_count);
    push_u16_le(&mut out, entry_count);
    push_u32_le(&mut out, central_size);
    push_u32_le(&mut out, central_offset);
    push_u16_le(&mut out, 0);

    Ok(out)
}

pub fn backup_json_from_zip_bytes(bytes: &[u8]) -> Result<String, String> {
    let mut offset = 0usize;
    while offset + 30 <= bytes.len() {
        let Some(signature) = read_u32_le(bytes, offset) else {
            break;
        };
        if signature != 0x0403_4b50 {
            break;
        }
        let compression = read_u16_le(bytes, offset + 8)
            .ok_or_else(|| "zip local header incomplete".to_string())?;
        let compressed_size = read_u32_le(bytes, offset + 18)
            .ok_or_else(|| "zip entry size missing".to_string())?
            as usize;
        let name_len = read_u16_le(bytes, offset + 26)
            .ok_or_else(|| "zip entry name missing".to_string())? as usize;
        let extra_len = read_u16_le(bytes, offset + 28)
            .ok_or_else(|| "zip extra length missing".to_string())?
            as usize;
        let name_start = offset + 30;
        let name_end = name_start + name_len;
        let data_start = name_end + extra_len;
        let data_end = data_start + compressed_size;
        if data_end > bytes.len() {
            return Err("zip entry content is incomplete".to_string());
        }
        let name = String::from_utf8_lossy(&bytes[name_start..name_end]);
        if name == "backup.json" {
            if compression != 0 {
                return Err("compressed zip backup entries are not supported".to_string());
            }
            return String::from_utf8(bytes[data_start..data_end].to_vec())
                .map_err(|_| "backup.json in zip is not UTF-8".to_string());
        }
        offset = data_end;
    }

    Err("backup.json not found in zip backup".to_string())
}

pub fn backup_json_from_path(path: &Path) -> Result<String, String> {
    match path.extension().and_then(|item| item.to_str()) {
        Some("json") => match fs::read_to_string(path) {
            Ok(content) => {
                if serde_json::from_str::<Value>(&content).is_ok() {
                    return Ok(content);
                }
                if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
                    if let Some(archive_name) = auto_backup_archive_file_name(file_name) {
                        let archive_path = path.with_file_name(archive_name);
                        if archive_path.exists() {
                            return backup_json_from_path(&archive_path);
                        }
                    }
                }
                Ok(content)
            }
            Err(err) => {
                if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
                    if let Some(archive_name) = auto_backup_archive_file_name(file_name) {
                        let archive_path = path.with_file_name(archive_name);
                        if archive_path.exists() {
                            return backup_json_from_path(&archive_path);
                        }
                    }
                }
                Err(format!("read auto backup file failed: {err}"))
            }
        },
        Some("zip") => {
            let bytes =
                fs::read(path).map_err(|err| format!("read auto backup zip failed: {err}"))?;
            backup_json_from_zip_bytes(&bytes)
        }
        _ => Err("unsupported auto backup file type".to_string()),
    }
}

pub fn collect_auto_backup_platforms_from_value(value: &Value) -> Vec<AutoBackupPlatformEntry> {
    let accounts = value
        .get("accounts")
        .filter(|item| item.is_object())
        .unwrap_or(value);
    let Some(platforms) = accounts.get("platforms").and_then(|item| item.as_object()) else {
        return Vec::new();
    };

    let mut result = Vec::new();
    for (platform, payload) in platforms {
        let exported_data = payload
            .get("exported_data")
            .or_else(|| payload.get("data"))
            .or_else(|| payload.get("accounts"));
        let account_count = payload
            .get("account_count")
            .and_then(|item| item.as_u64())
            .or_else(|| {
                exported_data
                    .and_then(|item| item.as_array())
                    .map(|items| items.len() as u64)
            })
            .unwrap_or(0);
        if account_count == 0 {
            continue;
        }
        result.push(AutoBackupPlatformEntry {
            platform: platform.clone(),
            account_count,
        });
    }
    result.sort_by(|left, right| left.platform.cmp(&right.platform));
    result
}

pub fn collect_auto_backup_platforms(json_content: &str) -> Vec<AutoBackupPlatformEntry> {
    serde_json::from_str::<Value>(json_content)
        .ok()
        .map(|value| collect_auto_backup_platforms_from_value(&value))
        .unwrap_or_default()
}

pub fn build_auto_backup_zip_bytes(file_name: &str, content: &str) -> Result<Vec<u8>, String> {
    let root = serde_json::from_str::<Value>(content)
        .map_err(|err| format!("auto backup JSON parse failed, cannot build zip: {err}"))?;
    let platforms = collect_auto_backup_platforms_from_value(&root);
    let manifest = json!({
        "schema": "cockpit-tools.auto-backup-archive",
        "version": 1,
        "source_file_name": file_name,
        "platforms": &platforms,
        "sections": root.get("sections").cloned().unwrap_or(Value::Null),
        "exported_at": root.get("exported_at").cloned().unwrap_or(Value::Null),
    });

    let mut entries = vec![
        ("backup.json".to_string(), content.as_bytes().to_vec()),
        (
            "manifest.json".to_string(),
            serde_json::to_vec_pretty(&manifest)
                .map_err(|err| format!("serialize zip manifest failed: {err}"))?,
        ),
    ];

    let platforms_map = root
        .get("accounts")
        .filter(|item| item.is_object())
        .and_then(|accounts| accounts.get("platforms"))
        .and_then(|item| item.as_object())
        .or_else(|| root.get("platforms").and_then(|item| item.as_object()));
    if let Some(platforms_map) = platforms_map {
        for platform in &platforms {
            if let Some(payload) = platforms_map.get(&platform.platform) {
                let exported_data = payload
                    .get("exported_data")
                    .or_else(|| payload.get("data"))
                    .or_else(|| payload.get("accounts"))
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                entries.push((
                    format!("accounts/{}.json", platform.platform),
                    serde_json::to_vec_pretty(&exported_data)
                        .map_err(|err| format!("serialize platform backup failed: {err}"))?,
                ));
            }
        }
    }

    build_stored_zip(entries)
}

pub fn write_auto_backup_file(file_name: &str, content: &str) -> Result<String, String> {
    let safe_name = sanitize_auto_backup_file_name(file_name)?;
    if !safe_name.ends_with(".json") {
        return Err("auto backup main file must be JSON".to_string());
    }
    let dir = ensure_auto_backup_dir_path()?;
    let path = dir.join(&safe_name);
    atomic_write::write_string_atomic(&path, content)
        .map_err(|err| format!("write auto backup file failed: {err}"))?;

    if let Some(archive_name) = auto_backup_archive_file_name(&safe_name) {
        let archive_path = dir.join(archive_name);
        let zip_bytes = build_auto_backup_zip_bytes(&safe_name, content)?;
        atomic_write::write_bytes_atomic(&archive_path, &zip_bytes)
            .map_err(|err| format!("write auto backup zip failed: {err}"))?;
    }

    Ok(path.to_string_lossy().to_string())
}

pub fn read_auto_backup_file(file_name: &str) -> Result<String, String> {
    let path = auto_backup_file_path(file_name)?;
    backup_json_from_path(&path)
}

pub fn copy_auto_backup_file(file_name: &str, target_path: &str) -> Result<String, String> {
    let source_path = auto_backup_file_path(file_name)?;
    if !source_path.exists() {
        return Err("backup file does not exist".to_string());
    }
    let target = PathBuf::from(target_path.trim());
    if target.as_os_str().is_empty() {
        return Err("target path cannot be empty".to_string());
    }
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| format!("create target dir failed: {err}"))?;
        }
    }
    fs::copy(&source_path, &target).map_err(|err| format!("copy backup file failed: {err}"))?;
    Ok(target.to_string_lossy().to_string())
}

fn unix_millis(value: SystemTime) -> Option<i64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

pub fn list_auto_backup_files() -> Result<Vec<AutoBackupFileEntry>, String> {
    let dir = auto_backup_dir_path()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries =
        fs::read_dir(&dir).map_err(|err| format!("read auto backup dir failed: {err}"))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("read auto backup entry failed: {err}"))?;
        let path = entry.path();
        if path.is_file() {
            paths.push(path);
        }
    }

    let mut json_stems = HashSet::new();
    for path in &paths {
        if let Some(stem) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".json"))
        {
            json_stems.insert(stem.to_string());
        }
    }

    let mut files = Vec::new();
    for path in paths {
        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) if name.ends_with(".json") || name.ends_with(".zip") => name.to_string(),
            _ => continue,
        };
        if file_name.ends_with(".zip") {
            if let Some(json_name) = auto_backup_json_file_name(&file_name) {
                if json_stems.contains(json_name.trim_end_matches(".json")) {
                    continue;
                }
            }
        }
        let metadata =
            fs::metadata(&path).map_err(|err| format!("read backup metadata failed: {err}"))?;
        let archive_name = if file_name.ends_with(".json") {
            auto_backup_archive_file_name(&file_name)
        } else {
            None
        };
        let archive_path = archive_name
            .as_ref()
            .map(|name| dir.join(name))
            .filter(|path| path.exists());
        let archive_metadata = archive_path
            .as_ref()
            .and_then(|path| fs::metadata(path).ok());
        let json_content = backup_json_from_path(&path).ok();
        files.push(AutoBackupFileEntry {
            file_name,
            path: path.to_string_lossy().to_string(),
            file_kind: path
                .extension()
                .and_then(|item| item.to_str())
                .unwrap_or("json")
                .to_string(),
            size_bytes: metadata.len(),
            modified_at_ms: metadata.modified().ok().and_then(unix_millis),
            archive_file_name: archive_path
                .as_ref()
                .and_then(|path| path.file_name().and_then(|name| name.to_str()))
                .map(ToString::to_string),
            archive_path: archive_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            archive_size_bytes: archive_metadata.map(|metadata| metadata.len()),
            platforms: json_content
                .as_deref()
                .map(collect_auto_backup_platforms)
                .unwrap_or_default(),
        });
    }

    files.sort_by(|left, right| {
        right
            .modified_at_ms
            .unwrap_or_default()
            .cmp(&left.modified_at_ms.unwrap_or_default())
            .then_with(|| right.file_name.cmp(&left.file_name))
    });
    Ok(files)
}

pub fn delete_auto_backup_file(file_name: &str) -> Result<(), String> {
    let path = auto_backup_file_path(file_name)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|err| format!("delete backup file failed: {err}"))?;
    }
    if file_name.ends_with(".json") {
        if let Some(archive_name) = auto_backup_archive_file_name(file_name) {
            let archive_path = auto_backup_file_path(&archive_name)?;
            if archive_path.exists() {
                fs::remove_file(&archive_path)
                    .map_err(|err| format!("delete backup zip failed: {err}"))?;
            }
        }
    }
    Ok(())
}

pub fn cleanup_auto_backup_files(retention_days: i32) -> Result<Vec<String>, String> {
    let dir = auto_backup_dir_path()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let retention_days = config::sanitize_auto_backup_retention_days(retention_days);
    let now = SystemTime::now();
    let cutoff = now
        .checked_sub(Duration::from_secs(retention_days as u64 * 24 * 60 * 60))
        .unwrap_or(now);

    let mut deleted = Vec::new();
    let entries =
        fs::read_dir(&dir).map_err(|err| format!("read auto backup dir failed: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read auto backup entry failed: {err}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) if name.ends_with(".json") || name.ends_with(".zip") => name.to_string(),
            _ => continue,
        };
        let metadata =
            fs::metadata(&path).map_err(|err| format!("read backup metadata failed: {err}"))?;
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified >= cutoff {
            continue;
        }
        fs::remove_file(&path).map_err(|err| format!("cleanup backup failed: {err}"))?;
        deleted.push(file_name);
    }

    deleted.sort();
    Ok(deleted)
}

pub fn open_auto_backup_dir() -> Result<(), String> {
    let path = ensure_auto_backup_dir_path()?;
    system_host::open_path_in_system(&path)
}

pub fn build_webdav_sync_settings(user_config: &config::UserConfig) -> WebdavSyncSettings {
    let url = webdav_sync::normalize_base_url(&user_config.webdav_sync_url)
        .unwrap_or_else(|_| config::default_webdav_sync_url());
    let remote_dir = webdav_sync::normalize_remote_dir(&user_config.webdav_sync_remote_dir)
        .unwrap_or_else(|_| config::default_webdav_sync_remote_dir());

    WebdavSyncSettings {
        enabled: user_config.webdav_sync_enabled,
        url,
        username: user_config.webdav_sync_username.clone(),
        has_password: !user_config.webdav_sync_password.is_empty(),
        remote_dir,
        last_upload_at: user_config.webdav_sync_last_upload_at.clone(),
        last_upload_file_name: user_config.webdav_sync_last_upload_file_name.clone(),
        last_download_at: user_config.webdav_sync_last_download_at.clone(),
        last_download_file_name: user_config.webdav_sync_last_download_file_name.clone(),
        retention_days: user_config.webdav_sync_retention_days,
    }
}

pub fn get_webdav_sync_settings() -> WebdavSyncSettings {
    let user_config = config::get_user_config();
    build_webdav_sync_settings(&user_config)
}

pub fn resolve_webdav_password_update(
    current_password: &str,
    password: Option<String>,
    clear_password: Option<bool>,
) -> String {
    if clear_password.unwrap_or(false) {
        return String::new();
    }
    password
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| current_password.to_string())
}

pub fn validate_webdav_sync_config(
    enabled: bool,
    url: &str,
    username: &str,
    password: &str,
    remote_dir: &str,
) -> Result<(String, String, String), String> {
    let normalized_url = webdav_sync::normalize_base_url(url)?;
    let normalized_remote_dir = webdav_sync::normalize_remote_dir(remote_dir)?;
    let normalized_username = username.trim().to_string();

    if enabled {
        if normalized_username.is_empty() {
            return Err("WebDAV username cannot be empty when sync is enabled".to_string());
        }
        if password.is_empty() {
            return Err("WebDAV app password cannot be empty when sync is enabled".to_string());
        }
    }

    Ok((normalized_url, normalized_username, normalized_remote_dir))
}

pub fn save_webdav_sync_settings(
    enabled: bool,
    url: &str,
    username: &str,
    password: Option<String>,
    clear_password: Option<bool>,
    remote_dir: &str,
    retention_days: Option<i32>,
) -> Result<WebdavSyncSettings, String> {
    let current = config::get_user_config();
    let next_password =
        resolve_webdav_password_update(&current.webdav_sync_password, password, clear_password);
    let (next_url, next_username, next_remote_dir) =
        validate_webdav_sync_config(enabled, url, username, &next_password, remote_dir)?;
    let next_retention_days = retention_days.unwrap_or(current.webdav_sync_retention_days);

    let next = config::UserConfig {
        webdav_sync_enabled: enabled,
        webdav_sync_url: next_url,
        webdav_sync_username: next_username,
        webdav_sync_password: next_password,
        webdav_sync_remote_dir: next_remote_dir,
        webdav_sync_retention_days: config::sanitize_webdav_sync_retention_days(
            next_retention_days,
        ),
        ..current
    };
    config::save_user_config(&next)?;
    Ok(build_webdav_sync_settings(&next))
}

pub async fn test_webdav_sync_connection(
    url: &str,
    username: &str,
    password: Option<String>,
    clear_password: Option<bool>,
    remote_dir: &str,
) -> Result<webdav_sync::WebdavTestResult, String> {
    let current = config::get_user_config();
    let next_password =
        resolve_webdav_password_update(&current.webdav_sync_password, password, clear_password);
    let connection = webdav_sync::connection_from_parts(url, username, &next_password, remote_dir)?;
    webdav_sync::test_connection(&connection).await
}

pub async fn upload_auto_backup_to_webdav(
    file_name: &str,
) -> Result<webdav_sync::WebdavUploadResult, String> {
    let user_config = config::get_user_config();
    if !user_config.webdav_sync_enabled {
        return Err("WebDAV sync is not enabled".to_string());
    }

    let connection = webdav_sync::connection_from_config(&user_config)?;
    let safe_name = sanitize_auto_backup_file_name(file_name)?;
    if !safe_name.ends_with(".json") {
        return Err("WebDAV sync entry file must be a JSON backup".to_string());
    }

    let archive_name = auto_backup_archive_file_name(&safe_name)
        .ok_or_else(|| "cannot resolve matching backup archive name".to_string())?;
    let archive_path = auto_backup_file_path(&archive_name)?;
    if !archive_path.exists() {
        return Err("local backup archive does not exist".to_string());
    }

    let archive_bytes = fs::read(&archive_path)
        .map_err(|err| format!("read local backup archive failed: {err}"))?;
    let sync_client = webdav_sync::WebdavSyncClient::new(&connection)?;

    let uploaded_file = sync_client
        .upload_backup_bytes(&archive_name, archive_bytes)
        .await?;
    let retention_days =
        config::sanitize_webdav_sync_retention_days(user_config.webdav_sync_retention_days);
    let deleted_files = sync_client.cleanup_remote_backups(retention_days).await?;
    let uploaded_at = Utc::now().to_rfc3339();
    let remote_dir = connection.remote_dir.clone();

    let next = config::UserConfig {
        webdav_sync_last_upload_at: Some(uploaded_at.clone()),
        webdav_sync_last_upload_file_name: Some(archive_name),
        ..user_config
    };
    config::save_user_config(&next)?;

    Ok(webdav_sync::WebdavUploadResult {
        uploaded_files: vec![uploaded_file],
        deleted_files,
        uploaded_at,
        remote_dir,
    })
}

pub async fn list_webdav_backup_files() -> Result<Vec<webdav_sync::WebdavBackupFileEntry>, String> {
    let user_config = config::get_user_config();
    let connection = webdav_sync::connection_from_config(&user_config)?;
    webdav_sync::list_remote_backups(&connection).await
}

pub async fn read_webdav_backup_file(file_name: &str) -> Result<String, String> {
    let user_config = config::get_user_config();
    let safe_name = sanitize_auto_backup_file_name(file_name)?;
    let connection = webdav_sync::connection_from_config(&user_config)?;

    let downloaded_at = Utc::now().to_rfc3339();
    let content = if safe_name.ends_with(".zip") {
        let bytes = webdav_sync::read_remote_backup_bytes(&connection, &safe_name).await?;
        backup_json_from_zip_bytes(&bytes)?
    } else if safe_name.ends_with(".json") {
        webdav_sync::read_remote_backup(&connection, &safe_name).await?
    } else {
        return Err("unsupported backup file type".to_string());
    };

    let next = config::UserConfig {
        webdav_sync_last_download_at: Some(downloaded_at),
        webdav_sync_last_download_file_name: Some(safe_name),
        ..user_config
    };
    config::save_user_config(&next)?;
    Ok(content)
}

pub async fn delete_webdav_backup_file(file_name: &str) -> Result<(), String> {
    let user_config = config::get_user_config();
    let safe_name = sanitize_auto_backup_file_name(file_name)?;
    let connection = webdav_sync::connection_from_config(&user_config)?;
    webdav_sync::delete_remote_backup(&connection, &safe_name).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_file_name_validation_rejects_paths() {
        assert!(sanitize_auto_backup_file_name("").is_err());
        assert!(sanitize_auto_backup_file_name("../backup.json").is_err());
        assert!(sanitize_auto_backup_file_name("backup.txt").is_err());
        assert_eq!(
            sanitize_auto_backup_file_name(" cockpit_auto_backup_1.json ").unwrap(),
            "cockpit_auto_backup_1.json"
        );
    }

    #[test]
    fn backup_zip_round_trips_json() {
        let content = r#"{"accounts":{"platforms":{"codex":{"account_count":2}}}}"#;
        let zip = build_auto_backup_zip_bytes("cockpit_auto_backup_1.json", content).unwrap();
        assert_eq!(backup_json_from_zip_bytes(&zip).unwrap(), content);
    }

    #[test]
    fn platform_summary_supports_legacy_and_current_shapes() {
        let content = json!({
            "accounts": {
                "platforms": {
                    "codex": { "data": [{}, {}] },
                    "zed": { "account_count": 1 },
                    "empty": { "data": [] }
                }
            }
        });
        let platforms = collect_auto_backup_platforms_from_value(&content);
        assert_eq!(platforms.len(), 2);
        assert_eq!(platforms[0].platform, "codex");
        assert_eq!(platforms[0].account_count, 2);
        assert_eq!(platforms[1].platform, "zed");
        assert_eq!(platforms[1].account_count, 1);
    }
}
