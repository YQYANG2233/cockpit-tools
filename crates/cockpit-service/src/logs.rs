use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::params::*;
use crate::rpc_types::ManagedLogFile;
use crate::LogSnapshot;

pub(crate) fn get_antigravity_installed_version_info(
    params: &Value,
) -> Result<
    Option<cockpit_core::modules::antigravity_runtime::AntigravityInstalledVersionInfo>,
    String,
> {
    let target = param_optional_string(params, &["target"])?;
    let scan_mode = param_optional_string(params, &["scanMode", "scan_mode"])?;
    let scan_mode =
        cockpit_core::modules::antigravity_runtime::normalize_antigravity_version_scan_mode(
            scan_mode.as_deref(),
        );
    let timeout_ms =
        cockpit_core::modules::antigravity_runtime::antigravity_version_timeout_ms(scan_mode);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result =
            cockpit_core::modules::antigravity_runtime::resolve_antigravity_installed_version_info_for_target_with_scan_mode(
                target.as_deref(),
                scan_mode,
            );
        let _ = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(result) => Ok(result),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(err) => Err(format!("Antigravity version detection failed: {err}")),
    }
}

pub(crate) fn downloads_dir() -> Result<String, String> {
    cockpit_core::modules::system_host::downloads_dir()
        .map(|path| path.to_string_lossy().to_string())
}

pub(crate) fn home_dir() -> Result<String, String> {
    cockpit_core::modules::system_host::home_dir().map(|path| path.to_string_lossy().to_string())
}

fn to_unix_millis(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
        .and_then(|value| i64::try_from(value).ok())
}

fn build_managed_log_file(path: &Path) -> Result<ManagedLogFile, String> {
    let metadata =
        fs::metadata(path).map_err(|err| format!("read log file metadata failed: {err}"))?;

    Ok(ManagedLogFile {
        log_file_path: path.to_string_lossy().to_string(),
        log_file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        file_size: metadata.len(),
        modified_at_ms: metadata.modified().ok().and_then(to_unix_millis),
    })
}

fn build_available_log_files(paths: Vec<PathBuf>) -> Result<Vec<ManagedLogFile>, String> {
    paths
        .into_iter()
        .map(|path| build_managed_log_file(path.as_path()))
        .collect()
}

pub(crate) fn logs_get_snapshot(params: &Value) -> Result<LogSnapshot, String> {
    let file_name = param_optional_string(params, &["fileName", "file_name"])?;
    let line_limit = param_optional_u64(params, &["lineLimit", "line_limit"])?
        .map(|value| usize::try_from(value).unwrap_or(usize::MAX));
    let line_limit = cockpit_core::modules::logger::clamp_log_tail_lines(line_limit);
    let log_dir = cockpit_core::modules::logger::get_log_dir()?;
    let log_file = cockpit_core::modules::logger::resolve_managed_log_file(file_name.as_deref())?;
    let content = cockpit_core::modules::logger::read_log_tail_lines(&log_file, line_limit)?;
    let metadata =
        fs::metadata(&log_file).map_err(|err| format!("read log file metadata failed: {err}"))?;
    let available_files =
        build_available_log_files(cockpit_core::modules::logger::list_managed_log_files()?)?;

    Ok(LogSnapshot {
        log_dir_path: log_dir.to_string_lossy().to_string(),
        log_file_path: log_file.to_string_lossy().to_string(),
        log_file_name: log_file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        content,
        line_limit,
        file_size: metadata.len(),
        modified_at_ms: metadata.modified().ok().and_then(to_unix_millis),
        available_files,
    })
}

pub(crate) fn logs_open_log_directory() -> Result<(), String> {
    let log_dir = cockpit_core::modules::logger::get_log_dir()?;
    crate::open_path_in_system(&log_dir)
}
