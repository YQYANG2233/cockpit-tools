use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use crate::modules;

const HISTORY_FILE: &str = "wakeup_history.json";
const MAX_HISTORY_ITEMS: usize = 100;

static HISTORY_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeupHistoryItem {
    pub id: String,
    pub timestamp: i64,
    pub trigger_type: String,
    pub trigger_source: String,
    pub task_name: Option<String>,
    pub account_email: String,
    pub model_id: String,
    pub prompt: Option<String>,
    pub success: bool,
    pub status: Option<String>,
    pub message: Option<String>,
    pub duration: Option<u64>,
}

fn history_path() -> Result<PathBuf, String> {
    Ok(modules::account::get_data_dir()?.join(HISTORY_FILE))
}

pub fn load_history() -> Result<Vec<WakeupHistoryItem>, String> {
    let path = history_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("read wakeup history failed: {error}"))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    match serde_json::from_str::<Vec<WakeupHistoryItem>>(&content) {
        Ok(items) => Ok(items),
        Err(error) => {
            match modules::atomic_write::quarantine_file(&path, "invalid-json") {
                Ok(Some(backup_path)) => modules::logger::log_warn(&format!(
                    "[WakeupHistory] invalid history quarantined: path={}, backup={}, error={}",
                    path.display(),
                    backup_path.display(),
                    error
                )),
                Ok(None) => modules::logger::log_warn(&format!(
                    "[WakeupHistory] invalid history disappeared before quarantine: path={}, error={}",
                    path.display(),
                    error
                )),
                Err(backup_error) => modules::logger::log_warn(&format!(
                    "[WakeupHistory] invalid history quarantine failed: path={}, parse_error={}, backup_error={}",
                    path.display(),
                    error,
                    backup_error
                )),
            }
            Ok(Vec::new())
        }
    }
}

fn save_history(items: &[WakeupHistoryItem]) -> Result<(), String> {
    let path = history_path()?;
    let content = serde_json::to_string_pretty(items)
        .map_err(|error| format!("serialize wakeup history failed: {error}"))?;
    modules::atomic_write::write_string_atomic(&path, &content)
        .map_err(|error| format!("save wakeup history failed: {error}"))
}

pub fn add_history_items(new_items: Vec<WakeupHistoryItem>) -> Result<(), String> {
    if new_items.is_empty() {
        return Ok(());
    }

    let _lock = HISTORY_LOCK
        .lock()
        .map_err(|_| "wakeup history lock poisoned".to_string())?;
    let mut existing = load_history().unwrap_or_default();
    let existing_ids = existing
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();

    let mut merged = new_items
        .into_iter()
        .filter(|item| !existing_ids.contains(&item.id))
        .collect::<Vec<_>>();
    if merged.is_empty() {
        return Ok(());
    }

    merged.append(&mut existing);
    merged.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    merged.truncate(MAX_HISTORY_ITEMS);
    save_history(&merged)
}

pub fn clear_history() -> Result<(), String> {
    let _lock = HISTORY_LOCK
        .lock()
        .map_err(|_| "wakeup history lock poisoned".to_string())?;
    save_history(&[])
}
