use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Duration;

use crate::params::*;
use crate::rpc_types::block_on;
use crate::sse::publish_event;

const WAKEUP_TASKS_FILE: &str = "wakeup_tasks.json";
const DEFAULT_WAKEUP_PROMPT: &str = "hi";

static WAKEUP_STATE: LazyLock<Mutex<WakeupSchedulerState>> =
    LazyLock::new(|| Mutex::new(WakeupSchedulerState::default()));
static WAKEUP_PENDING_CONFIRMATIONS: LazyLock<Mutex<HashMap<String, WakeupPendingConfirmation>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static WAKEUP_OFFICIAL_LS_VERSION_MODE: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeupTaskInput {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub last_run_at: Option<i64>,
    pub schedule: WakeupScheduleConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWakeupState {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub tasks: Vec<WakeupTaskInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeupScheduleConfig {
    pub repeat_mode: String,
    pub daily_times: Option<Vec<String>>,
    pub weekly_days: Option<Vec<i32>>,
    pub weekly_times: Option<Vec<String>>,
    pub interval_hours: Option<i32>,
    pub interval_start_time: Option<String>,
    pub interval_end_time: Option<String>,
    pub selected_models: Vec<String>,
    pub selected_accounts: Vec<String>,
    pub crontab: Option<String>,
    pub wake_on_reset: Option<bool>,
    pub custom_prompt: Option<String>,
    pub max_output_tokens: Option<i32>,
    pub time_window_enabled: Option<bool>,
    pub time_window_start: Option<String>,
    pub time_window_end: Option<String>,
    pub fallback_times: Option<Vec<String>>,
    pub startup_delay_minutes: Option<i32>,
    pub execution_mode: Option<String>,
    pub confirm_timeout_minutes: Option<i32>,
}

#[derive(Debug, Default, Clone)]
struct WakeupSchedulerState {
    enabled: bool,
    tasks: Vec<WakeupTaskInput>,
}

#[derive(Debug, Clone)]
struct WakeupPendingConfirmation {
    task: WakeupTaskInput,
    trigger_source: String,
    timeout_at: i64,
}

#[derive(Debug, Clone)]
struct WakeupCronField {
    _values: HashSet<i32>,
}

pub(crate) fn set_wakeup_override(params: &Value) -> Result<(), String> {
    let enabled = param_bool(params, &["enabled"])?;
    cockpit_core::modules::websocket::broadcast_wakeup_override(enabled);
    Ok(())
}

fn wakeup_tasks_path() -> Result<PathBuf, String> {
    Ok(cockpit_core::modules::account::get_data_dir()?.join(WAKEUP_TASKS_FILE))
}

fn write_pretty_json_atomic<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value)
        .map_err(|err| format!("serialize json failed: {err}"))?;
    cockpit_core::modules::atomic_write::write_string_atomic(path, &content)
        .map_err(|err| format!("write {} failed: {err}", path.display()))
}

fn normalize_wakeup_task(mut task: WakeupTaskInput) -> WakeupTaskInput {
    if task.name.trim().is_empty() {
        task.name = task.id.clone();
    }
    let schedule = &mut task.schedule;
    schedule.repeat_mode = match schedule.repeat_mode.as_str() {
        "daily" | "weekly" | "interval" => schedule.repeat_mode.clone(),
        _ => "daily".to_string(),
    };
    if schedule.daily_times.as_ref().is_none_or(Vec::is_empty) {
        schedule.daily_times = Some(vec!["08:00".to_string()]);
    }
    if schedule.weekly_days.as_ref().is_none_or(Vec::is_empty) {
        schedule.weekly_days = Some(vec![1, 2, 3, 4, 5]);
    }
    if schedule.weekly_times.as_ref().is_none_or(Vec::is_empty) {
        schedule.weekly_times = Some(vec!["08:00".to_string()]);
    }
    schedule.interval_hours = Some(schedule.interval_hours.unwrap_or(4).max(1));
    if schedule
        .interval_start_time
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        schedule.interval_start_time = Some("07:00".to_string());
    }
    if schedule
        .interval_end_time
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        schedule.interval_end_time = Some("22:00".to_string());
    }
    schedule.max_output_tokens = Some(schedule.max_output_tokens.unwrap_or(0).max(0));
    if schedule.fallback_times.as_ref().is_none_or(Vec::is_empty) {
        schedule.fallback_times = Some(vec!["07:00".to_string()]);
    }
    schedule.startup_delay_minutes = schedule
        .startup_delay_minutes
        .map(|value| value.clamp(0, 24 * 60));
    schedule.execution_mode = Some(match schedule.execution_mode.as_deref() {
        Some("confirm") => "confirm".to_string(),
        _ => "auto".to_string(),
    });
    schedule.confirm_timeout_minutes =
        Some(schedule.confirm_timeout_minutes.unwrap_or(5).clamp(1, 60));
    task
}

pub(crate) fn sync_wakeup_state(params: &Value) -> Result<(), String> {
    let enabled = param_bool(params, &["enabled"])?;
    let tasks = param_json::<Vec<WakeupTaskInput>>(params, &["tasks"])?
        .into_iter()
        .map(normalize_wakeup_task)
        .collect::<Vec<_>>();
    set_wakeup_official_ls_version_mode(params)?;

    let persisted = PersistedWakeupState {
        enabled,
        tasks: tasks.clone(),
    };
    write_pretty_json_atomic(&wakeup_tasks_path()?, &persisted)?;

    let mut state = WAKEUP_STATE
        .lock()
        .map_err(|_| "wakeup state lock poisoned".to_string())?;
    state.enabled = enabled;
    state.tasks = tasks;
    Ok(())
}

fn load_wakeup_state_from_disk() -> PersistedWakeupState {
    let path = match wakeup_tasks_path() {
        Ok(path) => path,
        Err(_) => return PersistedWakeupState::default(),
    };
    if !path.exists() {
        return PersistedWakeupState::default();
    }
    let Ok(content) = fs::read_to_string(&path) else {
        return PersistedWakeupState::default();
    };
    if content.trim().is_empty() {
        return PersistedWakeupState::default();
    }
    match serde_json::from_str::<PersistedWakeupState>(&content) {
        Ok(mut state) => {
            state.tasks = state.tasks.into_iter().map(normalize_wakeup_task).collect();
            state
        }
        Err(err) => {
            let _ = cockpit_core::modules::atomic_write::quarantine_file(&path, "invalid-json");
            cockpit_core::modules::logger::log_warn(&format!(
                "[cockpit-service] wakeup task file is invalid and was quarantined: {err}"
            ));
            PersistedWakeupState::default()
        }
    }
}

fn wakeup_current_state() -> Result<WakeupSchedulerState, String> {
    let mut state = WAKEUP_STATE
        .lock()
        .map_err(|_| "wakeup state lock poisoned".to_string())?;
    if state.tasks.is_empty() {
        let persisted = load_wakeup_state_from_disk();
        state.enabled = persisted.enabled;
        state.tasks = persisted.tasks;
    }
    Ok(state.clone())
}

pub(crate) fn load_wakeup_history(
) -> Result<Vec<cockpit_core::modules::wakeup_history::WakeupHistoryItem>, String> {
    cockpit_core::modules::wakeup_history::load_history()
}

pub(crate) fn add_wakeup_history(params: &Value) -> Result<(), String> {
    let new_items = param_json::<Vec<cockpit_core::modules::wakeup_history::WakeupHistoryItem>>(
        params,
        &["items"],
    )?;
    cockpit_core::modules::wakeup_history::add_history_items(new_items)
}

pub(crate) fn clear_wakeup_history() -> Result<(), String> {
    cockpit_core::modules::wakeup_history::clear_history()
}

pub(crate) fn set_wakeup_official_ls_version_mode(params: &Value) -> Result<(), String> {
    let mode = param_optional_string(
        params,
        &["mode", "officialLsVersionMode", "official_ls_version_mode"],
    )?;
    cockpit_core::modules::wakeup::set_official_ls_version_mode(mode.as_deref())?;
    let mut guard = WAKEUP_OFFICIAL_LS_VERSION_MODE
        .lock()
        .map_err(|_| "wakeup official LS version mode lock poisoned".to_string())?;
    *guard = mode;
    Ok(())
}

pub(crate) fn ensure_wakeup_runtime_ready(params: &Value) -> Result<Option<String>, String> {
    set_wakeup_official_ls_version_mode(params)?;
    cockpit_core::modules::wakeup::ensure_wakeup_runtime_ready()
}

pub(crate) fn fetch_wakeup_available_models(
) -> Result<Vec<cockpit_core::modules::wakeup::AvailableModel>, String> {
    block_on(cockpit_core::modules::wakeup::fetch_available_models())
}

pub(crate) fn validate_wakeup_crontab(params: &Value) -> Result<(), String> {
    let expr = param_string(params, &["expr"])?;
    parse_wakeup_crontab_expression(&expr).map(|_| ())
}

fn parse_wakeup_crontab_expression(expr: &str) -> Result<[WakeupCronField; 5], String> {
    let parts = expr.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 5 {
        return Err(
            "crontab expression must contain five fields: minute hour day month weekday"
                .to_string(),
        );
    }
    Ok([
        parse_wakeup_cron_field(parts[0], 0, 59, false, "minute")?,
        parse_wakeup_cron_field(parts[1], 0, 23, false, "hour")?,
        parse_wakeup_cron_field(parts[2], 1, 31, false, "day")?,
        parse_wakeup_cron_field(parts[3], 1, 12, false, "month")?,
        parse_wakeup_cron_field(parts[4], 0, 7, true, "weekday")?,
    ])
}

fn parse_wakeup_cron_field(
    field: &str,
    min: i32,
    max: i32,
    normalize_weekday: bool,
    label: &str,
) -> Result<WakeupCronField, String> {
    let trimmed = field.trim();
    if trimmed.is_empty() {
        return Err(format!("crontab {label} field cannot be empty"));
    }

    let mut values = HashSet::new();
    for segment in trimmed.split(',') {
        parse_wakeup_cron_segment(
            segment.trim(),
            min,
            max,
            normalize_weekday,
            label,
            &mut values,
        )?;
    }
    if values.is_empty() {
        return Err(format!("crontab {label} field has no usable value"));
    }
    Ok(WakeupCronField { _values: values })
}

fn parse_wakeup_cron_segment(
    segment: &str,
    min: i32,
    max: i32,
    normalize_weekday: bool,
    label: &str,
    out: &mut HashSet<i32>,
) -> Result<(), String> {
    if segment.is_empty() {
        return Err(format!("crontab {label} field contains an empty segment"));
    }
    let (range_part, step) = if let Some((left, right)) = segment.split_once('/') {
        let step = right
            .trim()
            .parse::<i32>()
            .map_err(|_| format!("crontab {label} step is invalid: {}", right.trim()))?;
        if step <= 0 {
            return Err(format!("crontab {label} step must be greater than 0"));
        }
        (left.trim(), step)
    } else {
        (segment, 1)
    };

    if range_part == "*" {
        insert_wakeup_cron_range(out, min, max, step, normalize_weekday);
        return Ok(());
    }

    if let Some((start_raw, end_raw)) = range_part.split_once('-') {
        let start = parse_wakeup_cron_number(start_raw, label)?;
        let end = parse_wakeup_cron_number(end_raw, label)?;
        validate_wakeup_cron_value(start, min, max, normalize_weekday, label)?;
        validate_wakeup_cron_value(end, min, max, normalize_weekday, label)?;
        if end < start {
            return Err(format!("crontab {label} range is invalid: {start}-{end}"));
        }
        insert_wakeup_cron_range(out, start, end, step, normalize_weekday);
        return Ok(());
    }

    let value = parse_wakeup_cron_number(range_part, label)?;
    validate_wakeup_cron_value(value, min, max, normalize_weekday, label)?;
    if step == 1 {
        out.insert(normalize_wakeup_cron_value(value, normalize_weekday));
    } else {
        insert_wakeup_cron_range(out, value, max, step, normalize_weekday);
    }
    Ok(())
}

fn parse_wakeup_cron_number(raw: &str, label: &str) -> Result<i32, String> {
    raw.trim()
        .parse::<i32>()
        .map_err(|_| format!("crontab {label} value is invalid: {}", raw.trim()))
}

fn validate_wakeup_cron_value(
    value: i32,
    min: i32,
    max: i32,
    normalize_weekday: bool,
    label: &str,
) -> Result<(), String> {
    if normalize_weekday && value == 7 {
        return Ok(());
    }
    if value < min || value > max {
        return Err(format!(
            "crontab {label} value is out of range ({min}-{max}): {value}"
        ));
    }
    Ok(())
}

fn normalize_wakeup_cron_value(value: i32, normalize_weekday: bool) -> i32 {
    if normalize_weekday && value == 7 {
        0
    } else {
        value
    }
}

fn insert_wakeup_cron_range(
    out: &mut HashSet<i32>,
    start: i32,
    end: i32,
    step: i32,
    normalize_weekday: bool,
) {
    let mut value = start;
    while value <= end {
        out.insert(normalize_wakeup_cron_value(value, normalize_weekday));
        value += step;
    }
}

pub(crate) fn wakeup_trigger(
    params: &Value,
) -> Result<cockpit_core::modules::wakeup::WakeupResponse, String> {
    let account_id = param_string(params, &["accountId", "account_id"])?;
    let model = param_string(params, &["model"])?;
    let prompt = param_optional_string(params, &["prompt"])?
        .unwrap_or_else(|| DEFAULT_WAKEUP_PROMPT.to_string());
    let max_output_tokens =
        param_optional_u64(params, &["maxOutputTokens", "max_output_tokens"])?.unwrap_or(0);
    let max_output_tokens = u32::try_from(max_output_tokens)
        .map_err(|_| "maxOutputTokens must fit in u32".to_string())?;
    let cancel_scope_id = param_optional_string(params, &["cancelScopeId", "cancel_scope_id"])?;
    set_wakeup_official_ls_version_mode(params)?;
    block_on(cockpit_core::modules::wakeup::trigger_wakeup(
        &account_id,
        &model,
        &prompt,
        max_output_tokens,
        cancel_scope_id.as_deref(),
    ))
}

pub(crate) fn run_wakeup_enabled_tasks(params: &Value) -> Result<u32, String> {
    set_wakeup_official_ls_version_mode(params)?;
    let trigger_source = param_optional_string(params, &["triggerSource", "trigger_source"])?
        .unwrap_or_else(|| "startup".to_string());
    let state = wakeup_current_state()?;
    if !state.enabled {
        return Ok(0);
    }

    let mut pending = WAKEUP_PENDING_CONFIRMATIONS
        .lock()
        .map_err(|_| "wakeup pending confirmation lock poisoned".to_string())?;
    let now = chrono::Local::now().timestamp();
    let mut started = 0u32;
    for task in state.tasks.iter().filter(|task| task.enabled) {
        if task.schedule.execution_mode.as_deref() == Some("confirm") {
            let timeout_minutes = task
                .schedule
                .confirm_timeout_minutes
                .unwrap_or(5)
                .clamp(1, 60);
            pending.insert(
                task.id.clone(),
                WakeupPendingConfirmation {
                    task: task.clone(),
                    trigger_source: trigger_source.clone(),
                    timeout_at: now + timeout_minutes as i64 * 60,
                },
            );
            started = started.saturating_add(1);
        }
    }
    Ok(started)
}

pub(crate) fn wakeup_cancel_scope(params: &Value) -> Result<(), String> {
    let cancel_scope_id = param_string(params, &["cancelScopeId", "cancel_scope_id"])?;
    cockpit_core::modules::wakeup::cancel_wakeup_scope(&cancel_scope_id)
}

pub(crate) fn wakeup_release_scope(params: &Value) -> Result<(), String> {
    let cancel_scope_id = param_string(params, &["cancelScopeId", "cancel_scope_id"])?;
    cockpit_core::modules::wakeup::release_wakeup_scope(&cancel_scope_id)
}

fn wakeup_record_status_history(
    task: &WakeupTaskInput,
    trigger_source: &str,
    status: &str,
) -> Result<(), String> {
    let item = cockpit_core::modules::wakeup_history::WakeupHistoryItem {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        trigger_type: "scheduled".to_string(),
        trigger_source: trigger_source.to_string(),
        task_name: Some(task.name.clone()),
        account_email: task
            .schedule
            .selected_accounts
            .first()
            .cloned()
            .unwrap_or_default(),
        model_id: task
            .schedule
            .selected_models
            .first()
            .cloned()
            .unwrap_or_default(),
        prompt: task.schedule.custom_prompt.clone(),
        success: status == "success",
        status: Some(status.to_string()),
        message: Some(format!("Status: {status}")),
        duration: Some(0),
    };
    cockpit_core::modules::wakeup_history::add_history_items(vec![item])
}

pub(crate) fn wakeup_verification_load_state(
) -> Result<Vec<cockpit_core::modules::wakeup_verification::WakeupVerificationStateItem>, String> {
    cockpit_core::modules::wakeup_verification::build_display_state_for_all_accounts()
}

pub(crate) fn wakeup_verification_load_history() -> Result<
    Vec<cockpit_core::modules::wakeup_verification::WakeupVerificationBatchHistoryItem>,
    String,
> {
    cockpit_core::modules::wakeup_verification::load_history()
}

pub(crate) fn wakeup_verification_delete_history(params: &Value) -> Result<usize, String> {
    let batch_ids = param_string_vec(params, &["batchIds", "batch_ids"])?;
    cockpit_core::modules::wakeup_verification::delete_history(batch_ids)
}

pub(crate) fn wakeup_verification_run_batch(
    params: &Value,
) -> Result<cockpit_core::modules::wakeup_verification::WakeupVerificationBatchResult, String> {
    let account_ids = param_string_vec(params, &["accountIds", "account_ids"])?;
    let model = param_string(params, &["model"])?;
    let prompt = param_optional_string(params, &["prompt"])?
        .unwrap_or_else(|| DEFAULT_WAKEUP_PROMPT.to_string());
    let max_output_tokens =
        param_optional_u64(params, &["maxOutputTokens", "max_output_tokens"])?.unwrap_or(0);
    let max_output_tokens = u32::try_from(max_output_tokens)
        .map_err(|_| "maxOutputTokens must fit in u32".to_string())?;
    set_wakeup_official_ls_version_mode(params)?;

    block_on(cockpit_core::modules::wakeup_verification::run_batch(
        account_ids,
        &model,
        &prompt,
        max_output_tokens,
        |payload| {
            let data = serde_json::to_value(payload).unwrap_or(Value::Null);
            publish_event("wakeup://verification-progress", data);
        },
    ))
}

// ---------------------------------------------------------------------------
// Codex wakeup functions
// ---------------------------------------------------------------------------

pub(crate) fn codex_wakeup_get_cli_status(
) -> Result<cockpit_core::modules::codex_wakeup::CodexCliStatus, String> {
    Ok(cockpit_core::modules::codex_wakeup::get_cli_status())
}

pub(crate) fn codex_wakeup_update_runtime_config(
    params: &Value,
) -> Result<cockpit_core::modules::codex_wakeup::CodexCliStatus, String> {
    let codex_cli_path = param_optional_string(params, &["codexCliPath", "codex_cli_path"])?;
    let node_path = param_optional_string(params, &["nodePath", "node_path"])?;
    cockpit_core::modules::codex_wakeup::save_runtime_config(
        &cockpit_core::modules::codex_wakeup::CodexWakeupRuntimeConfig {
            codex_cli_path,
            node_path,
        },
    )?;
    Ok(cockpit_core::modules::codex_wakeup::get_cli_status())
}

pub(crate) fn codex_wakeup_get_overview(
) -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupOverview, String> {
    cockpit_core::modules::codex_wakeup::load_overview()
}

pub(crate) fn codex_wakeup_get_state(
) -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupState, String> {
    cockpit_core::modules::codex_wakeup::load_state()
}

pub(crate) fn codex_wakeup_save_state(
    params: &Value,
) -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupState, String> {
    let enabled = param_bool(params, &["enabled"])?;
    let tasks = param_json::<Vec<cockpit_core::modules::codex_wakeup::CodexWakeupTask>>(
        params,
        &["tasks"],
    )?;
    let model_presets = param_json::<
        Vec<cockpit_core::modules::codex_wakeup::CodexWakeupModelPreset>,
    >(params, &["modelPresets", "model_presets"])?;
    let model_preset_migrations = param_string_vec(
        params,
        &["modelPresetMigrations", "model_preset_migrations"],
    )?;

    cockpit_core::modules::codex_wakeup::save_state(
        &cockpit_core::modules::codex_wakeup::CodexWakeupState {
            enabled,
            tasks,
            model_presets,
            model_preset_migrations,
        },
    )
}

pub(crate) fn codex_wakeup_load_history(
) -> Result<Vec<cockpit_core::modules::codex_wakeup::CodexWakeupHistoryItem>, String> {
    cockpit_core::modules::codex_wakeup::load_history()
}

pub(crate) fn codex_wakeup_clear_history() -> Result<(), String> {
    cockpit_core::modules::codex_wakeup::clear_history()
}

pub(crate) fn codex_wakeup_cancel_scope(params: &Value) -> Result<(), String> {
    let cancel_scope_id = param_string(params, &["cancelScopeId", "cancel_scope_id"])?;
    cockpit_core::modules::codex_wakeup::cancel_wakeup_scope(&cancel_scope_id)
}

pub(crate) fn codex_wakeup_release_scope(params: &Value) -> Result<(), String> {
    let cancel_scope_id = param_string(params, &["cancelScopeId", "cancel_scope_id"])?;
    cockpit_core::modules::codex_wakeup::release_wakeup_scope(&cancel_scope_id)
}

fn publish_codex_wakeup_progress(
    payload: cockpit_core::modules::codex_wakeup::CodexWakeupProgressPayload,
) {
    let data = serde_json::to_value(payload).unwrap_or(Value::Null);
    publish_event(cockpit_core::modules::codex_wakeup::PROGRESS_EVENT, data);
}

fn run_codex_wakeup_batch(
    account_ids: Vec<String>,
    prompt: Option<String>,
    execution_config: cockpit_core::modules::codex_wakeup::CodexWakeupExecutionConfig,
    context: cockpit_core::modules::codex_wakeup::TaskRunContext,
    run_id: Option<String>,
    cancel_scope_id: Option<&str>,
) -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupBatchResult, String> {
    block_on(cockpit_core::modules::codex_wakeup::run_batch(
        account_ids,
        prompt,
        execution_config,
        context,
        run_id,
        cancel_scope_id,
        publish_codex_wakeup_progress,
    ))
}

pub(crate) fn codex_wakeup_test(
    params: &Value,
) -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupBatchResult, String> {
    let account_ids = param_string_vec(params, &["accountIds", "account_ids"])?;
    let prompt = param_optional_string(params, &["prompt"])?;
    let model = param_optional_string(params, &["model"])?;
    let model_display_name =
        param_optional_string(params, &["modelDisplayName", "model_display_name"])?;
    let model_reasoning_effort =
        param_optional_string(params, &["modelReasoningEffort", "model_reasoning_effort"])?;
    let run_id = param_optional_string(params, &["runId", "run_id"])?;
    let cancel_scope_id = param_optional_string(params, &["cancelScopeId", "cancel_scope_id"])?;

    run_codex_wakeup_batch(
        account_ids,
        prompt,
        cockpit_core::modules::codex_wakeup::CodexWakeupExecutionConfig {
            model,
            model_display_name,
            model_reasoning_effort,
        },
        cockpit_core::modules::codex_wakeup::TaskRunContext {
            trigger_type: "test".to_string(),
            task_id: None,
            task_name: None,
        },
        run_id,
        cancel_scope_id.as_deref(),
    )
}

fn run_codex_wakeup_task_by_id(
    task_id: &str,
    trigger_type: &str,
    run_id: Option<String>,
) -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupBatchResult, String> {
    block_on(cockpit_core::modules::codex_wakeup::run_task_now(
        task_id,
        trigger_type,
        run_id,
        publish_codex_wakeup_progress,
    ))
}

pub(crate) fn codex_wakeup_run_task(
    params: &Value,
) -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupBatchResult, String> {
    let task_id = param_string(params, &["taskId", "task_id"])?;
    let run_id = param_optional_string(params, &["runId", "run_id"])?;
    run_codex_wakeup_task_by_id(&task_id, "manual_task", run_id)
}

pub(crate) fn codex_wakeup_run_enabled_tasks(params: &Value) -> Result<u32, String> {
    let trigger = param_optional_string(params, &["triggerType", "trigger_type"])?
        .unwrap_or_else(|| "startup".to_string());
    let trigger = if trigger.trim().is_empty() {
        "startup".to_string()
    } else {
        trigger
    };

    let state = cockpit_core::modules::codex_wakeup::load_state_for_scheduler()?;
    if !state.enabled {
        return Ok(0);
    }

    if trigger == "startup" {
        let startup_tasks = state
            .tasks
            .into_iter()
            .filter(|task| task.enabled && task.schedule.kind == "startup")
            .map(|task| {
                (
                    task.id,
                    task.schedule.startup_delay_minutes.unwrap_or(0).max(0),
                )
            })
            .collect::<Vec<_>>();
        for (task_id, delay_minutes) in &startup_tasks {
            let task_id = task_id.clone();
            let delay_seconds = (*delay_minutes as u64).saturating_mul(60);
            thread::spawn(move || {
                if delay_seconds > 0 {
                    thread::sleep(Duration::from_secs(delay_seconds));
                }
                if let Err(err) = run_codex_wakeup_task_by_id(&task_id, "startup", None) {
                    cockpit_core::modules::logger::log_warn(&format!(
                        "[cockpit-service][CodexWakeup] startup task failed: task_id={task_id}, error={err}"
                    ));
                }
            });
        }
        return Ok(startup_tasks.len() as u32);
    }

    let mut started = 0u32;
    for task in state
        .tasks
        .into_iter()
        .filter(|task| task.enabled && task.schedule.kind != "startup")
    {
        match run_codex_wakeup_task_by_id(&task.id, &trigger, None) {
            Ok(_) => started = started.saturating_add(1),
            Err(err) => cockpit_core::modules::logger::log_warn(&format!(
                "[cockpit-service][CodexWakeup] enabled task failed: task_id={}, error={err}",
                task.id
            )),
        }
    }
    Ok(started)
}

pub(crate) fn confirm_wakeup_task(params: &Value) -> Result<(), String> {
    let task_id = param_string(params, &["taskId", "task_id"])?;
    let pending = WAKEUP_PENDING_CONFIRMATIONS
        .lock()
        .map_err(|_| "wakeup pending confirmation lock poisoned".to_string())?
        .remove(&task_id);

    if let Some(pending) = pending {
        let status = if chrono::Local::now().timestamp() > pending.timeout_at {
            "skipped_timeout"
        } else {
            "skipped_web_service_unmigrated"
        };
        wakeup_record_status_history(&pending.task, &pending.trigger_source, status)?;
    }
    Ok(())
}

pub(crate) fn cancel_wakeup_task(params: &Value) -> Result<(), String> {
    let task_id = param_string(params, &["taskId", "task_id"])?;
    WAKEUP_PENDING_CONFIRMATIONS
        .lock()
        .map_err(|_| "wakeup pending confirmation lock poisoned".to_string())?
        .remove(&task_id);
    Ok(())
}

pub(crate) fn check_wakeup_timeouts() -> Result<(), String> {
    let now = chrono::Local::now().timestamp();
    let expired = {
        let mut pending = WAKEUP_PENDING_CONFIRMATIONS
            .lock()
            .map_err(|_| "wakeup pending confirmation lock poisoned".to_string())?;
        let expired_ids = pending
            .iter()
            .filter(|(_, item)| now > item.timeout_at)
            .map(|(task_id, _)| task_id.clone())
            .collect::<Vec<_>>();
        expired_ids
            .into_iter()
            .filter_map(|task_id| pending.remove(&task_id))
            .collect::<Vec<_>>()
    };

    for pending in expired {
        wakeup_record_status_history(&pending.task, &pending.trigger_source, "skipped_timeout")?;
    }
    Ok(())
}
