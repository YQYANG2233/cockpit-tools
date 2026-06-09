use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Request, Response, ResponseBox, Server, StatusCode};
use toml_edit::{value as toml_value, Document};

mod rpc_catalog;

pub const DEFAULT_ADDR: &str = "127.0.0.1:19529";
pub const RPC_TOKEN_HEADER: &str = "x-cockpit-tools-rpc-token";
const TOKEN_ENV: &str = "COCKPIT_TOOLS_RPC_TOKEN";
const ADDR_ENV: &str = "COCKPIT_TOOLS_SERVICE_ADDR";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default = "default_jsonrpc")]
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitializeResult {
    pub version: String,
    pub service_addr: String,
    pub platform_family: &'static str,
    pub platform_os: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedLogFile {
    pub log_file_path: String,
    pub log_file_name: String,
    pub file_size: u64,
    pub modified_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogSnapshot {
    pub log_dir_path: String,
    pub log_file_path: String,
    pub log_file_name: String,
    pub content: String,
    pub line_limit: usize,
    pub file_size: u64,
    pub modified_at_ms: Option<i64>,
    pub available_files: Vec<ManagedLogFile>,
}

pub use cockpit_core::modules::auto_backup::{
    AutoBackupFileEntry, AutoBackupPlatformEntry, AutoBackupSettings, WebdavSyncSettings,
};

const CODEX_CONTEXT_WINDOW_1M_VALUE: i64 = 1_000_000;
const CODEX_AUTO_COMPACT_DEFAULT_LIMIT: i64 = 900_000;
const CODEX_CONFIG_MODEL_CONTEXT_WINDOW_KEY: &str = "model_context_window";
const CODEX_CONFIG_MODEL_AUTO_COMPACT_TOKEN_LIMIT_KEY: &str = "model_auto_compact_token_limit";
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

static EVENT_SUBSCRIBERS: LazyLock<Mutex<Vec<mpsc::Sender<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
const SSE_FLUSH_PADDING_BYTES: usize = 8193;

#[derive(Debug, Clone)]
struct AntigravityOAuthState {
    auth_url: String,
    redirect_uri: String,
    expected_state: String,
    code: Option<String>,
}

static ANTIGRAVITY_OAUTH_STATE: LazyLock<Mutex<Option<AntigravityOAuthState>>> =
    LazyLock::new(|| Mutex::new(None));
static CODEX_OAUTH_LISTENER_LOGIN_ID: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

struct EventStreamReader {
    receiver: mpsc::Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    offset: usize,
}

impl EventStreamReader {
    fn new(receiver: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            buffer: Vec::new(),
            offset: 0,
        }
    }
}

impl Read for EventStreamReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        while self.offset >= self.buffer.len() {
            match self.receiver.recv() {
                Ok(next) => {
                    self.buffer = next;
                    self.offset = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let count = out.len().min(self.buffer.len() - self.offset);
        out[..count].copy_from_slice(&self.buffer[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

fn default_jsonrpc() -> String {
    "2.0".to_string()
}

pub fn rpc_auth_token() -> String {
    std::env::var(TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "cockpit-tools-local-dev-token".to_string())
}

pub fn resolve_addr_from_env() -> String {
    std::env::var(ADDR_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_ADDR.to_string())
}

fn ok(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

fn error(id: Value, code: i32, message: impl Into<String>, data: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
            data,
        }),
    }
}

fn value_or_rpc_error<T: Serialize>(id: Value, result: Result<T, String>) -> JsonRpcResponse {
    match result {
        Ok(value) => ok(id, serde_json::to_value(value).unwrap_or(Value::Null)),
        Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
    }
}

fn to_value_result<T: Serialize>(result: Result<T, String>) -> Result<Value, String> {
    result.and_then(|value| {
        serde_json::to_value(value).map_err(|err| format!("serialize result failed: {err}"))
    })
}

fn block_on<T, F>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("create async runtime failed: {err}"))?
        .block_on(future)
}

fn block_on_value<T, F>(future: F) -> Result<T, String>
where
    F: Future<Output = T>,
{
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("create async runtime failed: {err}"))?
        .block_on(future))
}

fn param_string(params: &Value, keys: &[&str]) -> Result<String, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }
    Err(format!("missing string param: {}", keys.join("|")))
}

fn param_string_or_empty(params: &Value, keys: &[&str]) -> Result<String, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(String::new());
            }
            if let Some(value) = value.as_str() {
                return Ok(value.to_string());
            }
        }
    }
    Err(format!("missing string param: {}", keys.join("|")))
}

fn param_optional_string(params: &Value, keys: &[&str]) -> Result<Option<String>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            if let Some(value) = value.as_str() {
                let trimmed = value.trim();
                return Ok((!trimmed.is_empty()).then_some(trimmed.to_string()));
            }
            return Err(format!("{key} must be a string or null"));
        }
    }
    Ok(None)
}

fn param_nullable_string_presence(
    params: &Value,
    keys: &[&str],
) -> Result<Option<Option<String>>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(Some(None));
            }
            if let Some(value) = value.as_str() {
                let trimmed = value.trim();
                return Ok(Some((!trimmed.is_empty()).then_some(trimmed.to_string())));
            }
            return Err(format!("{key} must be a string or null"));
        }
    }
    Ok(None)
}

fn param_bool(params: &Value, keys: &[&str]) -> Result<bool, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if let Some(value) = value.as_bool() {
                return Ok(value);
            }
            if let Some(value) = value.as_str() {
                return match value.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" => Ok(true),
                    "false" | "0" | "no" => Ok(false),
                    _ => Err(format!("{key} must be a boolean")),
                };
            }
            return Err(format!("{key} must be a boolean"));
        }
    }
    Err(format!("missing boolean param: {}", keys.join("|")))
}

fn param_optional_bool(params: &Value, keys: &[&str]) -> Result<Option<bool>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            return param_bool(params, &[*key]).map(Some);
        }
    }
    Ok(None)
}

fn param_u64(params: &Value, keys: &[&str]) -> Result<u64, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if let Some(value) = value.as_u64() {
                return Ok(value);
            }
            if let Some(value) = value.as_str() {
                return value
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| format!("{key} must be an unsigned integer"));
            }
            return Err(format!("{key} must be an unsigned integer"));
        }
    }
    Err(format!("missing integer param: {}", keys.join("|")))
}

fn param_optional_u64(params: &Value, keys: &[&str]) -> Result<Option<u64>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            return param_u64(params, &[*key]).map(Some);
        }
    }
    Ok(None)
}

fn param_u32(params: &Value, keys: &[&str]) -> Result<u32, String> {
    let value = param_u64(params, keys)?;
    u32::try_from(value).map_err(|_| format!("{} must fit in u32", keys.join("|")))
}

fn param_u16(params: &Value, keys: &[&str]) -> Result<u16, String> {
    let value = param_u64(params, keys)?;
    u16::try_from(value).map_err(|_| format!("{} must be a valid port", keys.join("|")))
}

fn param_optional_u16(params: &Value, keys: &[&str]) -> Result<Option<u16>, String> {
    match param_optional_u64(params, keys)? {
        Some(value) => u16::try_from(value)
            .map(Some)
            .map_err(|_| format!("{} must be a valid port", keys.join("|"))),
        None => Ok(None),
    }
}

fn param_optional_i64(params: &Value, keys: &[&str]) -> Result<Option<i64>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            if let Some(value) = value.as_i64() {
                return Ok(Some(value));
            }
            if let Some(value) = value.as_str() {
                return value
                    .trim()
                    .parse::<i64>()
                    .map(Some)
                    .map_err(|_| format!("{key} must be an integer"));
            }
            return Err(format!("{key} must be an integer"));
        }
    }
    Ok(None)
}

fn param_i32(params: &Value, keys: &[&str]) -> Result<i32, String> {
    let value = param_optional_i64(params, keys)?
        .ok_or_else(|| format!("missing integer param: {}", keys.join("|")))?;
    i32::try_from(value).map_err(|_| format!("{} must fit in i32", keys.join("|")))
}

fn param_string_vec(params: &Value, keys: &[&str]) -> Result<Vec<String>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(values) = obj.get(*key).and_then(Value::as_array) {
            return values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                        .ok_or_else(|| format!("{key} must contain only non-empty strings"))
                })
                .collect();
        }
    }
    Err(format!("missing string array param: {}", keys.join("|")))
}

fn param_optional_string_vec(params: &Value, keys: &[&str]) -> Result<Option<Vec<String>>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            if let Some(values) = value.as_array() {
                let parsed = values
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(ToString::to_string)
                            .ok_or_else(|| format!("{key} must contain only non-empty strings"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(Some(parsed));
            }
            return Err(format!("{key} must be a string array or null"));
        }
    }
    Ok(None)
}

fn param_optional_tray_layout_groups(
    params: &Value,
) -> Result<Option<Vec<cockpit_core::modules::tray_layout::TrayLayoutGroup>>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in ["platformGroups", "platform_groups"] {
        if let Some(value) = obj.get(key) {
            if value.is_null() {
                return Ok(None);
            }
            return serde_json::from_value(value.clone())
                .map(Some)
                .map_err(|err| format!("{key} must be a tray layout group array: {err}"));
        }
    }
    Ok(None)
}

fn param_string_map(params: &Value, key: &str) -> Result<HashMap<String, String>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    let Some(value) = obj.get(key) else {
        return Ok(HashMap::new());
    };
    serde_json::from_value(value.clone())
        .map_err(|err| format!("{key} must be a string map: {err}"))
}

fn param_optional_bool_map(params: &Value, keys: &[&str]) -> Result<HashMap<String, bool>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(HashMap::new());
            }
            return serde_json::from_value(value.clone())
                .map_err(|err| format!("{key} must be a boolean map: {err}"));
        }
    }
    Ok(HashMap::new())
}

fn param_json<T: DeserializeOwned>(params: &Value, keys: &[&str]) -> Result<T, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            return serde_json::from_value(value.clone())
                .map_err(|err| format!("{} is invalid: {err}", keys.join("|")));
        }
    }
    Err(format!("missing param: {}", keys.join("|")))
}

fn param_optional_json<T: DeserializeOwned>(
    params: &Value,
    keys: &[&str],
) -> Result<Option<T>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            return serde_json::from_value(value.clone())
                .map(Some)
                .map_err(|err| format!("{} is invalid: {err}", keys.join("|")));
        }
    }
    Ok(None)
}

fn param_optional_codex_api_provider_mode(
    params: &Value,
) -> Result<Option<cockpit_core::models::codex::CodexApiProviderMode>, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    for key in ["apiProviderMode", "api_provider_mode"] {
        if let Some(value) = obj.get(key) {
            if value.is_null() {
                return Ok(None);
            }
            return serde_json::from_value(value.clone())
                .map(Some)
                .map_err(|err| format!("{key} is invalid: {err}"));
        }
    }
    Ok(None)
}

fn param_codex_app_speed(
    params: &Value,
) -> Result<cockpit_core::models::codex::CodexAppSpeed, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    let value = obj
        .get("speed")
        .ok_or_else(|| "missing speed param".to_string())?;
    serde_json::from_value(value.clone()).map_err(|err| format!("speed is invalid: {err}"))
}

fn param_account_id(params: &Value) -> Result<String, String> {
    param_string(params, &["accountId", "account_id"])
}

fn param_account_ids(params: &Value) -> Result<Vec<String>, String> {
    param_string_vec(params, &["accountIds", "account_ids"])
}

fn param_json_content(params: &Value) -> Result<String, String> {
    param_string(params, &["jsonContent", "json_content"])
}

fn param_access_token(params: &Value) -> Result<String, String> {
    param_string(
        params,
        &[
            "accessToken",
            "access_token",
            "githubAccessToken",
            "github_access_token",
        ],
    )
}

fn param_login_id(params: &Value) -> Result<String, String> {
    param_string(params, &["loginId", "login_id"])
}

fn param_optional_login_id(params: &Value) -> Result<Option<String>, String> {
    param_optional_string(params, &["loginId", "login_id"])
}

fn param_callback_url(params: &Value) -> Result<String, String> {
    param_string(params, &["callbackUrl", "callback_url"])
}

fn param_data(params: &Value) -> Result<String, String> {
    param_string(params, &["data"])
}

fn param_tags(params: &Value) -> Result<Vec<String>, String> {
    param_string_vec(params, &["tags"])
}

fn count_success<T>(results: Vec<(String, Result<T, String>)>) -> i32 {
    results.iter().filter(|(_, result)| result.is_ok()).count() as i32
}

fn maybe_one<T: Serialize>(
    result: Result<Option<T>, String>,
    missing: &str,
) -> Result<Value, String> {
    match result? {
        Some(value) => serde_json::to_value(vec![value])
            .map_err(|err| format!("serialize import result failed: {err}")),
        None => Err(missing.to_string()),
    }
}

fn load_data_file(file_name: &str, default: &str) -> Result<String, String> {
    let path = cockpit_core::modules::account::get_data_dir()?.join(file_name);
    if !path.exists() {
        return Ok(default.to_string());
    }
    std::fs::read_to_string(&path).map_err(|err| format!("read {file_name} failed: {err}"))
}

fn save_data_file(file_name: &str, data: &str) -> Result<(), String> {
    let dir = cockpit_core::modules::account::get_data_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|err| format!("create data dir failed: {err}"))?;
    }
    let path = dir.join(file_name);
    std::fs::write(&path, data).map_err(|err| format!("write {file_name} failed: {err}"))
}

fn app_version() -> String {
    serde_json::from_str::<Value>(include_str!("../../../src-tauri/tauri.conf.json"))
        .ok()
        .and_then(|value| {
            value
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

fn updater_plugin_config(
) -> Result<cockpit_core::modules::linux_updater::UpdaterPluginConfig, String> {
    let config = serde_json::from_str::<Value>(include_str!("../../../src-tauri/tauri.conf.json"))
        .map_err(|err| format!("parse tauri config failed: {err}"))?;
    let updater = config
        .get("plugins")
        .and_then(|plugins| plugins.get("updater"))
        .cloned()
        .ok_or_else(|| "Updater plugin config is missing".to_string())?;
    serde_json::from_value(updater)
        .map_err(|err| format!("Failed to parse updater plugin config: {err}"))
}

pub fn application_version() -> String {
    app_version()
}

fn load_update_settings() -> Result<cockpit_core::modules::update_checker::UpdateSettings, String> {
    cockpit_core::modules::update_checker::load_update_settings()
}

fn save_update_settings(
    settings: &cockpit_core::modules::update_checker::UpdateSettings,
) -> Result<(), String> {
    cockpit_core::modules::update_checker::save_update_settings(settings)
}

fn update_last_check_time() -> Result<(), String> {
    cockpit_core::modules::update_checker::update_last_check_time()
}

fn should_check_updates() -> Result<bool, String> {
    let settings = load_update_settings()?;
    Ok(cockpit_core::modules::update_checker::should_check_for_updates(&settings))
}

fn check_version_jump(
) -> Result<Option<cockpit_core::modules::update_checker::VersionJumpInfo>, String> {
    cockpit_core::modules::update_checker::check_version_jump_for_version(&app_version())
}

fn update_log(params: &Value) -> Result<(), String> {
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

fn save_pending_update_notes_values(
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

fn save_pending_update_notes(params: &Value) -> Result<(), String> {
    let version = param_string(params, &["version"])?;
    let release_notes = param_string_or_empty(params, &["releaseNotes", "release_notes"])?;
    let release_notes_zh = param_string_or_empty(params, &["releaseNotesZh", "release_notes_zh"])?;
    save_pending_update_notes_values(version, release_notes, release_notes_zh)
}

fn get_release_history(params: &Value) -> Result<Value, String> {
    let locale = param_optional_string(params, &["locale"])?;
    let limit =
        param_optional_u64(params, &["limit"])?.and_then(|value| usize::try_from(value).ok());
    to_value_result(cockpit_core::modules::update_checker::get_release_history(
        locale.as_deref(),
        limit,
    ))
}

fn get_network_config() -> Result<cockpit_core::modules::config::NetworkConfig, String> {
    Ok(cockpit_core::modules::config::get_network_config(None))
}

fn save_network_config(params: &Value) -> Result<bool, String> {
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

fn set_app_path(params: &Value) -> Result<(), String> {
    let app = param_string(params, &["app"])?;
    let path = param_string_or_empty(params, &["path"])?;
    cockpit_core::modules::config::set_app_path(&app, path)
}

fn set_codex_launch_on_switch(params: &Value) -> Result<(), String> {
    let enabled = param_bool(params, &["enabled"])?;
    cockpit_core::modules::config::set_codex_launch_on_switch(enabled)
}

fn set_codex_local_access_entry_visible(params: &Value) -> Result<(), String> {
    let enabled = param_bool(params, &["enabled"])?;
    cockpit_core::modules::config::set_codex_local_access_entry_visible(enabled)
}

fn handle_desktop_shell_window_close(params: &Value) -> Result<(), String> {
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

fn set_floating_card_always_on_top(params: &Value) -> Result<(), String> {
    let always_on_top = param_bool(params, &["alwaysOnTop", "always_on_top"])?;
    cockpit_core::modules::config::set_floating_card_always_on_top(always_on_top)
}

fn set_floating_card_confirm_on_close(params: &Value) -> Result<(), String> {
    let confirm_on_close = param_bool(params, &["confirmOnClose", "confirm_on_close"])?;
    cockpit_core::modules::config::set_floating_card_confirm_on_close(confirm_on_close)
}

fn save_floating_card_position(params: &Value) -> Result<(), String> {
    let x = param_i32(params, &["x"])?;
    let y = param_i32(params, &["y"])?;
    cockpit_core::modules::config::save_floating_card_position(x, y)
}

fn set_wakeup_override(params: &Value) -> Result<(), String> {
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

fn sync_wakeup_state(params: &Value) -> Result<(), String> {
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

fn load_wakeup_history(
) -> Result<Vec<cockpit_core::modules::wakeup_history::WakeupHistoryItem>, String> {
    cockpit_core::modules::wakeup_history::load_history()
}

fn add_wakeup_history(params: &Value) -> Result<(), String> {
    let new_items = param_json::<Vec<cockpit_core::modules::wakeup_history::WakeupHistoryItem>>(
        params,
        &["items"],
    )?;
    cockpit_core::modules::wakeup_history::add_history_items(new_items)
}

fn clear_wakeup_history() -> Result<(), String> {
    cockpit_core::modules::wakeup_history::clear_history()
}

fn set_wakeup_official_ls_version_mode(params: &Value) -> Result<(), String> {
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

fn ensure_wakeup_runtime_ready(params: &Value) -> Result<Option<String>, String> {
    set_wakeup_official_ls_version_mode(params)?;
    cockpit_core::modules::wakeup::ensure_wakeup_runtime_ready()
}

fn fetch_wakeup_available_models(
) -> Result<Vec<cockpit_core::modules::wakeup::AvailableModel>, String> {
    block_on(cockpit_core::modules::wakeup::fetch_available_models())
}

fn validate_wakeup_crontab(params: &Value) -> Result<(), String> {
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

fn wakeup_trigger(params: &Value) -> Result<cockpit_core::modules::wakeup::WakeupResponse, String> {
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

fn run_wakeup_enabled_tasks(params: &Value) -> Result<u32, String> {
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

fn wakeup_cancel_scope(params: &Value) -> Result<(), String> {
    let cancel_scope_id = param_string(params, &["cancelScopeId", "cancel_scope_id"])?;
    cockpit_core::modules::wakeup::cancel_wakeup_scope(&cancel_scope_id)
}

fn wakeup_release_scope(params: &Value) -> Result<(), String> {
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

fn wakeup_verification_load_state(
) -> Result<Vec<cockpit_core::modules::wakeup_verification::WakeupVerificationStateItem>, String> {
    cockpit_core::modules::wakeup_verification::build_display_state_for_all_accounts()
}

fn wakeup_verification_load_history() -> Result<
    Vec<cockpit_core::modules::wakeup_verification::WakeupVerificationBatchHistoryItem>,
    String,
> {
    cockpit_core::modules::wakeup_verification::load_history()
}

fn wakeup_verification_delete_history(params: &Value) -> Result<usize, String> {
    let batch_ids = param_string_vec(params, &["batchIds", "batch_ids"])?;
    cockpit_core::modules::wakeup_verification::delete_history(batch_ids)
}

fn wakeup_verification_run_batch(
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

fn codex_wakeup_get_cli_status(
) -> Result<cockpit_core::modules::codex_wakeup::CodexCliStatus, String> {
    Ok(cockpit_core::modules::codex_wakeup::get_cli_status())
}

fn codex_wakeup_update_runtime_config(
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

fn codex_wakeup_get_overview(
) -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupOverview, String> {
    cockpit_core::modules::codex_wakeup::load_overview()
}

fn codex_wakeup_get_state() -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupState, String>
{
    cockpit_core::modules::codex_wakeup::load_state()
}

fn codex_wakeup_save_state(
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

fn codex_wakeup_load_history(
) -> Result<Vec<cockpit_core::modules::codex_wakeup::CodexWakeupHistoryItem>, String> {
    cockpit_core::modules::codex_wakeup::load_history()
}

fn codex_wakeup_clear_history() -> Result<(), String> {
    cockpit_core::modules::codex_wakeup::clear_history()
}

fn codex_wakeup_cancel_scope(params: &Value) -> Result<(), String> {
    let cancel_scope_id = param_string(params, &["cancelScopeId", "cancel_scope_id"])?;
    cockpit_core::modules::codex_wakeup::cancel_wakeup_scope(&cancel_scope_id)
}

fn codex_wakeup_release_scope(params: &Value) -> Result<(), String> {
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

fn codex_wakeup_test(
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

fn codex_wakeup_run_task(
    params: &Value,
) -> Result<cockpit_core::modules::codex_wakeup::CodexWakeupBatchResult, String> {
    let task_id = param_string(params, &["taskId", "task_id"])?;
    let run_id = param_optional_string(params, &["runId", "run_id"])?;
    run_codex_wakeup_task_by_id(&task_id, "manual_task", run_id)
}

fn codex_wakeup_run_enabled_tasks(params: &Value) -> Result<u32, String> {
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

fn confirm_wakeup_task(params: &Value) -> Result<(), String> {
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

fn cancel_wakeup_task(params: &Value) -> Result<(), String> {
    let task_id = param_string(params, &["taskId", "task_id"])?;
    WAKEUP_PENDING_CONFIRMATIONS
        .lock()
        .map_err(|_| "wakeup pending confirmation lock poisoned".to_string())?
        .remove(&task_id);
    Ok(())
}

fn check_wakeup_timeouts() -> Result<(), String> {
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

fn external_import_fetch_import_url(params: &Value) -> Result<String, String> {
    let import_url = param_string(params, &["importUrl", "import_url"])?;
    block_on(cockpit_core::modules::external_import::fetch_external_import_url(&import_url))
}

fn external_import_submit_url(params: &Value) -> Result<Value, String> {
    let raw_url = param_string(params, &["rawUrl", "raw_url", "url"])?;
    let source =
        param_optional_string(params, &["source"])?.unwrap_or_else(|| "web-gateway".to_string());
    let mut payload =
        cockpit_core::modules::external_import::parse_external_import_url_with_reason(&raw_url)?;
    payload.source = Some(source);
    payload.raw_url = Some(raw_url);
    cockpit_core::modules::external_import::set_pending_external_import(payload.clone());
    let value =
        serde_json::to_value(&payload).map_err(|err| format!("serialize payload failed: {err}"))?;
    publish_event(
        cockpit_core::modules::external_import::EXTERNAL_PROVIDER_IMPORT_EVENT,
        value.clone(),
    );
    Ok(value)
}

fn save_tray_platform_layout(
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

fn detect_app_path(params: &Value) -> Result<Option<String>, String> {
    let app = param_string(params, &["app"])?;
    let force = param_optional_bool(params, &["force"])?.unwrap_or(false);
    cockpit_core::modules::system_host::detect_app_path(&app, force)
}

fn get_antigravity_installed_version_info(
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

fn downloads_dir() -> Result<String, String> {
    cockpit_core::modules::system_host::downloads_dir()
        .map(|path| path.to_string_lossy().to_string())
}

fn home_dir() -> Result<String, String> {
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

fn logs_get_snapshot(params: &Value) -> Result<LogSnapshot, String> {
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

fn logs_open_log_directory() -> Result<(), String> {
    let log_dir = cockpit_core::modules::logger::get_log_dir()?;
    open_path_in_system(&log_dir)
}

fn auto_backup_retention_days(params: &Value) -> Result<i32, String> {
    let value = param_optional_i64(params, &["retentionDays", "retention_days"])?.unwrap_or(3);
    let value = i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX });
    Ok(cockpit_core::modules::config::sanitize_auto_backup_retention_days(value))
}

fn get_auto_backup_settings() -> Result<AutoBackupSettings, String> {
    cockpit_core::modules::auto_backup::get_auto_backup_settings()
}

fn save_auto_backup_settings(params: &Value) -> Result<AutoBackupSettings, String> {
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

fn update_auto_backup_last_run(params: &Value) -> Result<AutoBackupSettings, String> {
    let last_backup_at = param_optional_string(params, &["lastBackupAt", "last_backup_at"])?;
    cockpit_core::modules::auto_backup::update_auto_backup_last_run(last_backup_at)
}

fn write_auto_backup_file(params: &Value) -> Result<String, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    let content = param_string_or_empty(params, &["content"])?;
    cockpit_core::modules::auto_backup::write_auto_backup_file(&file_name, &content)
}

fn read_auto_backup_file(params: &Value) -> Result<String, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    cockpit_core::modules::auto_backup::read_auto_backup_file(&file_name)
}

fn copy_auto_backup_file(params: &Value) -> Result<String, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    let target_path = param_string(params, &["targetPath", "target_path"])?;
    cockpit_core::modules::auto_backup::copy_auto_backup_file(&file_name, &target_path)
}

fn list_auto_backup_files() -> Result<Vec<AutoBackupFileEntry>, String> {
    cockpit_core::modules::auto_backup::list_auto_backup_files()
}

fn delete_auto_backup_file(params: &Value) -> Result<(), String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    cockpit_core::modules::auto_backup::delete_auto_backup_file(&file_name)
}

fn cleanup_auto_backup_files(params: &Value) -> Result<Vec<String>, String> {
    let retention_days = auto_backup_retention_days(params)?;
    cockpit_core::modules::auto_backup::cleanup_auto_backup_files(retention_days)
}

fn open_auto_backup_dir() -> Result<(), String> {
    cockpit_core::modules::auto_backup::open_auto_backup_dir()
}

fn get_webdav_sync_settings() -> WebdavSyncSettings {
    cockpit_core::modules::auto_backup::get_webdav_sync_settings()
}

fn save_webdav_sync_settings(params: &Value) -> Result<WebdavSyncSettings, String> {
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

fn test_webdav_sync_connection(
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

fn upload_auto_backup_to_webdav(
    params: &Value,
) -> Result<cockpit_core::modules::webdav_sync::WebdavUploadResult, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    block_on(cockpit_core::modules::auto_backup::upload_auto_backup_to_webdav(&file_name))
}

fn list_webdav_backup_files(
) -> Result<Vec<cockpit_core::modules::webdav_sync::WebdavBackupFileEntry>, String> {
    block_on(cockpit_core::modules::auto_backup::list_webdav_backup_files())
}

fn read_webdav_backup_file(params: &Value) -> Result<String, String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    block_on(cockpit_core::modules::auto_backup::read_webdav_backup_file(
        &file_name,
    ))
}

fn delete_webdav_backup_file(params: &Value) -> Result<(), String> {
    let file_name = param_string(params, &["fileName", "file_name"])?;
    block_on(cockpit_core::modules::auto_backup::delete_webdav_backup_file(&file_name))
}

fn data_transfer_apply_user_config(params: &Value) -> Result<bool, String> {
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

fn data_transfer_get_instance_store(
    params: &Value,
) -> Result<cockpit_core::models::InstanceStore, String> {
    let platform = param_string(params, &["platform"])?;
    cockpit_core::modules::data_transfer::load_instance_store_by_platform(platform.trim())
}

fn data_transfer_replace_instance_store(params: &Value) -> Result<(), String> {
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

fn save_text_file(params: &Value) -> Result<(), String> {
    let path = param_string(params, &["path"])?;
    let content = param_string_or_empty(params, &["content"])?;
    cockpit_core::modules::system_host::save_text_file(path, &content)
}

fn read_text_file(params: &Value) -> Result<String, String> {
    let path = param_string(params, &["path"])?;
    cockpit_core::modules::system_host::read_text_file(path)
}

fn open_path_in_system(path: impl AsRef<Path>) -> Result<(), String> {
    cockpit_core::modules::system_host::open_path_in_system(path)
}

fn open_system_path(params: &Value) -> Result<(), String> {
    let path = param_string(params, &["path"])?;
    cockpit_core::modules::system_host::open_existing_path(path)
}

fn open_data_folder() -> Result<(), String> {
    cockpit_core::modules::system_host::open_data_folder()
}

fn open_folder(params: &Value) -> Result<(), String> {
    let path = param_string(params, &["path"])?;
    cockpit_core::modules::system_host::open_folder(path)
}

fn codex_config_toml_path() -> String {
    cockpit_core::modules::codex_account::get_codex_home()
        .join("config.toml")
        .to_string_lossy()
        .to_string()
}

fn open_codex_config_toml() -> Result<(), String> {
    let path = cockpit_core::modules::codex_account::get_codex_home().join("config.toml");
    if !path.exists() {
        return Err(format!("Codex config.toml not found: {}", path.display()));
    }
    open_path_in_system(&path)
}

fn write_codex_quick_config_to_dir(
    base_dir: &Path,
    model_context_window: Option<i64>,
    auto_compact_token_limit: Option<i64>,
) -> Result<Value, String> {
    let config_path = base_dir.join("config.toml");
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let context_window_1m =
        model_context_window.unwrap_or_default() >= CODEX_CONTEXT_WINDOW_1M_VALUE;

    if existing.trim().is_empty() && !context_window_1m {
        return to_value_result(
            cockpit_core::modules::codex_account::read_quick_config_from_config_toml(base_dir),
        );
    }

    let mut doc = if existing.trim().is_empty() {
        Document::new()
    } else {
        existing
            .parse::<Document>()
            .map_err(|err| format!("parse config.toml failed: {err}"))?
    };

    if context_window_1m {
        let compact_limit = auto_compact_token_limit.unwrap_or(CODEX_AUTO_COMPACT_DEFAULT_LIMIT);
        if compact_limit <= 0 {
            return Err("auto compact token limit must be greater than 0".to_string());
        }
        doc[CODEX_CONFIG_MODEL_CONTEXT_WINDOW_KEY] = toml_value(CODEX_CONTEXT_WINDOW_1M_VALUE);
        doc[CODEX_CONFIG_MODEL_AUTO_COMPACT_TOKEN_LIMIT_KEY] = toml_value(compact_limit);
    } else {
        let _ = doc.remove(CODEX_CONFIG_MODEL_CONTEXT_WINDOW_KEY);
        let _ = doc.remove(CODEX_CONFIG_MODEL_AUTO_COMPACT_TOKEN_LIMIT_KEY);
    }

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create config dir failed: {err}"))?;
    }
    fs::write(&config_path, doc.to_string())
        .map_err(|err| format!("write config.toml failed: {err}"))?;
    to_value_result(
        cockpit_core::modules::codex_account::read_quick_config_from_config_toml(base_dir),
    )
}

fn save_codex_quick_config(params: &Value) -> Result<Value, String> {
    let model_context_window =
        param_optional_i64(params, &["modelContextWindow", "model_context_window"])?;
    let auto_compact_token_limit = param_optional_i64(
        params,
        &["autoCompactTokenLimit", "auto_compact_token_limit"],
    )?;
    write_codex_quick_config_to_dir(
        &cockpit_core::modules::codex_account::get_codex_home(),
        model_context_window,
        auto_compact_token_limit,
    )
}

fn save_codex_api_service_app_speed(params: &Value) -> Result<Value, String> {
    let speed = param_codex_app_speed(params)?;
    let saved = cockpit_core::modules::codex_speed::save_api_service_app_speed(speed.clone())?;
    if let Ok(settings) = cockpit_core::modules::codex_instance::load_default_settings() {
        if settings.bind_account_id.as_deref()
            == Some(cockpit_core::modules::codex_instance::CODEX_API_SERVICE_BIND_ACCOUNT_ID)
        {
            let _ = cockpit_core::modules::codex_instance::update_default_app_speed(speed);
        }
    }
    to_value_result(Ok::<_, String>(saved))
}

fn test_codex_model_provider_connection(params: &Value) -> Result<Value, String> {
    let base_url = param_string_or_empty(params, &["baseUrl", "base_url"])?;
    let api_key = param_string_or_empty(params, &["apiKey", "api_key"])?;
    let wire_api = param_optional_string(params, &["wireApi", "wire_api"])?;
    to_value_result(block_on(
        cockpit_core::modules::codex_model_provider::test_connection(base_url, api_key, wire_api),
    ))
}

fn query_codex_model_provider_usage(params: &Value) -> Result<Value, String> {
    let base_url = param_string_or_empty(params, &["baseUrl", "base_url"])?;
    let api_key = param_string_or_empty(params, &["apiKey", "api_key"])?;
    let integration_type = param_optional_string(params, &["integrationType", "integration_type"])?;
    to_value_result(block_on(
        cockpit_core::modules::codex_model_provider::query_usage(
            base_url,
            api_key,
            integration_type,
        ),
    ))
}

fn param_optional_launch_mode(
    params: &Value,
) -> Result<Option<cockpit_core::models::InstanceLaunchMode>, String> {
    let Some(raw) = param_optional_string(params, &["launchMode", "launch_mode"])? else {
        return Ok(None);
    };
    serde_json::from_value(Value::String(raw))
        .map(Some)
        .map_err(|err| format!("invalid launch mode: {err}"))
}

fn create_codex_instance(params: &Value) -> Result<Value, String> {
    let payload = cockpit_core::modules::codex_instance::CreateInstanceParams {
        name: param_string(params, &["name"])?,
        user_data_dir: param_string(params, &["userDataDir", "user_data_dir"])?,
        working_dir: param_optional_string(params, &["workingDir", "working_dir"])?,
        extra_args: param_optional_string(params, &["extraArgs", "extra_args"])?
            .unwrap_or_default(),
        bind_account_id: param_optional_string(params, &["bindAccountId", "bind_account_id"])?,
        copy_source_instance_id: param_optional_string(
            params,
            &["copySourceInstanceId", "copy_source_instance_id"],
        )?,
        init_mode: param_optional_string(params, &["initMode", "init_mode"])?,
        launch_mode: param_optional_launch_mode(params)?,
        app_speed: param_optional_json(params, &["appSpeed", "app_speed"])?,
    };
    to_value_result(cockpit_core::modules::codex_instance::create_instance(
        payload,
    ))
}

fn update_codex_instance(params: &Value) -> Result<Value, String> {
    let payload = cockpit_core::modules::codex_instance::UpdateInstanceParams {
        instance_id: param_string(params, &["instanceId", "instance_id"])?,
        name: param_optional_string(params, &["name"])?,
        working_dir: param_nullable_string_presence(params, &["workingDir", "working_dir"])?
            .flatten(),
        extra_args: param_optional_string(params, &["extraArgs", "extra_args"])?,
        bind_account_id: param_nullable_string_presence(
            params,
            &["bindAccountId", "bind_account_id"],
        )?,
        launch_mode: param_optional_launch_mode(params)?,
        app_speed: param_optional_json(params, &["appSpeed", "app_speed"])?,
    };
    to_value_result(cockpit_core::modules::codex_instance::update_instance(
        payload,
    ))
}

fn codex_instance_profile(
    instance_id: &str,
) -> Result<cockpit_core::models::InstanceProfile, String> {
    cockpit_core::modules::codex_instance::load_instance_store()?
        .instances
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| "instance not found".to_string())
}

fn get_codex_instance_quick_config(params: &Value) -> Result<Value, String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;
    let instance = codex_instance_profile(&instance_id)?;
    to_value_result(
        cockpit_core::modules::codex_account::read_quick_config_from_config_toml(Path::new(
            &instance.user_data_dir,
        )),
    )
}

fn save_codex_instance_quick_config(params: &Value) -> Result<Value, String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;
    let instance = codex_instance_profile(&instance_id)?;
    let model_context_window =
        param_optional_i64(params, &["modelContextWindow", "model_context_window"])?;
    let auto_compact_token_limit = param_optional_i64(
        params,
        &["autoCompactTokenLimit", "auto_compact_token_limit"],
    )?;
    write_codex_quick_config_to_dir(
        Path::new(&instance.user_data_dir),
        model_context_window,
        auto_compact_token_limit,
    )
}

fn open_codex_instance_config_toml(params: &Value) -> Result<(), String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;
    let instance = codex_instance_profile(&instance_id)?;
    let path = Path::new(&instance.user_data_dir).join("config.toml");
    if !path.exists() {
        return Err(format!(
            "Codex instance config.toml not found: {}",
            path.display()
        ));
    }
    open_path_in_system(&path)
}

const DEFAULT_INSTANCE_ID: &str = "__default__";

fn install_linux_update(params: &Value) -> Result<(), String> {
    let expected_version = param_optional_string(params, &["expectedVersion", "expected_version"])?;
    let updater_config = updater_plugin_config()?;
    block_on(cockpit_core::modules::linux_updater::install_linux_update(
        updater_config,
        &application_version(),
        expected_version,
        save_pending_update_notes_values,
        |payload| {
            let value = serde_json::to_value(payload).unwrap_or(Value::Null);
            publish_event(
                cockpit_core::modules::linux_updater::UPDATE_PROGRESS_EVENT,
                value,
            );
        },
    ))
}

fn load_default_settings_for_platform(
    platform: &str,
) -> Result<cockpit_core::models::DefaultInstanceSettings, String> {
    match platform {
        "antigravity" => cockpit_core::modules::instance::load_default_settings(),
        "codex" => cockpit_core::modules::codex_instance::load_default_settings(),
        "github_copilot" => cockpit_core::modules::github_copilot_instance::load_default_settings(),
        "windsurf" => cockpit_core::modules::windsurf_instance::load_default_settings(),
        "kiro" => cockpit_core::modules::kiro_instance::load_default_settings(),
        "cursor" => cockpit_core::modules::cursor_instance::load_default_settings(),
        "gemini" => cockpit_core::modules::gemini_instance::load_default_settings(),
        "codebuddy" => cockpit_core::modules::codebuddy_instance::load_default_settings(),
        "codebuddy_cn" => cockpit_core::modules::codebuddy_cn_instance::load_default_settings(),
        "qoder" => cockpit_core::modules::qoder_instance::load_default_settings(),
        "trae" => cockpit_core::modules::trae_instance::load_default_settings(),
        "workbuddy" => cockpit_core::modules::workbuddy_instance::load_default_settings(),
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn update_default_pid_for_platform(
    platform: &str,
    pid: Option<u32>,
) -> Result<cockpit_core::models::DefaultInstanceSettings, String> {
    match platform {
        "antigravity" => cockpit_core::modules::instance::update_default_pid(pid),
        "codex" => cockpit_core::modules::codex_instance::update_default_pid(pid),
        "github_copilot" => cockpit_core::modules::github_copilot_instance::update_default_pid(pid),
        "windsurf" => cockpit_core::modules::windsurf_instance::update_default_pid(pid),
        "kiro" => cockpit_core::modules::kiro_instance::update_default_pid(pid),
        "cursor" => cockpit_core::modules::cursor_instance::update_default_pid(pid),
        "gemini" => cockpit_core::modules::gemini_instance::update_default_pid(pid),
        "codebuddy" => cockpit_core::modules::codebuddy_instance::update_default_pid(pid),
        "codebuddy_cn" => cockpit_core::modules::codebuddy_cn_instance::update_default_pid(pid),
        "qoder" => cockpit_core::modules::qoder_instance::update_default_pid(pid),
        "trae" => cockpit_core::modules::trae_instance::update_default_pid(pid),
        "workbuddy" => cockpit_core::modules::workbuddy_instance::update_default_pid(pid),
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn update_instance_pid_for_platform(
    platform: &str,
    instance_id: &str,
    pid: Option<u32>,
) -> Result<cockpit_core::models::InstanceProfile, String> {
    match platform {
        "antigravity" => cockpit_core::modules::instance::update_instance_pid(instance_id, pid),
        "codex" => cockpit_core::modules::codex_instance::update_instance_pid(instance_id, pid),
        "github_copilot" => {
            cockpit_core::modules::github_copilot_instance::update_instance_pid(instance_id, pid)
        }
        "windsurf" => {
            cockpit_core::modules::windsurf_instance::update_instance_pid(instance_id, pid)
        }
        "kiro" => cockpit_core::modules::kiro_instance::update_instance_pid(instance_id, pid),
        "cursor" => cockpit_core::modules::cursor_instance::update_instance_pid(instance_id, pid),
        "gemini" => cockpit_core::modules::gemini_instance::update_instance_pid(instance_id, pid),
        "codebuddy" => {
            cockpit_core::modules::codebuddy_instance::update_instance_pid(instance_id, pid)
        }
        "codebuddy_cn" => {
            cockpit_core::modules::codebuddy_cn_instance::update_instance_pid(instance_id, pid)
        }
        "qoder" => cockpit_core::modules::qoder_instance::update_instance_pid(instance_id, pid),
        "trae" => cockpit_core::modules::trae_instance::update_instance_pid(instance_id, pid),
        "workbuddy" => {
            cockpit_core::modules::workbuddy_instance::update_instance_pid(instance_id, pid)
        }
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn clear_all_pids_for_platform(platform: &str) -> Result<(), String> {
    match platform {
        "antigravity" => cockpit_core::modules::instance::clear_all_pids(),
        "codex" => cockpit_core::modules::codex_instance::clear_all_pids(),
        "github_copilot" => cockpit_core::modules::github_copilot_instance::clear_all_pids(),
        "windsurf" => cockpit_core::modules::windsurf_instance::clear_all_pids(),
        "kiro" => cockpit_core::modules::kiro_instance::clear_all_pids(),
        "cursor" => cockpit_core::modules::cursor_instance::clear_all_pids(),
        "gemini" => cockpit_core::modules::gemini_instance::clear_all_pids(),
        "codebuddy" => cockpit_core::modules::codebuddy_instance::clear_all_pids(),
        "codebuddy_cn" => cockpit_core::modules::codebuddy_cn_instance::clear_all_pids(),
        "qoder" => cockpit_core::modules::qoder_instance::clear_all_pids(),
        "trae" => cockpit_core::modules::trae_instance::clear_all_pids(),
        "workbuddy" => cockpit_core::modules::workbuddy_instance::clear_all_pids(),
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn find_instance_for_platform(
    platform: &str,
    instance_id: &str,
) -> Result<cockpit_core::models::InstanceProfile, String> {
    load_instance_store_for_platform(platform)?
        .instances
        .into_iter()
        .find(|item| item.id == instance_id)
        .ok_or_else(|| "instance not found".to_string())
}

fn resolve_instance_pid_for_platform(
    platform: &str,
    last_pid: Option<u32>,
    user_data_dir: Option<&str>,
) -> Option<u32> {
    match platform {
        "antigravity" => {
            cockpit_core::modules::process::resolve_antigravity_pid(last_pid, user_data_dir)
        }
        "codex" => cockpit_core::modules::process::resolve_codex_pid(last_pid, user_data_dir),
        "github_copilot" => {
            cockpit_core::modules::process::resolve_vscode_pid(last_pid, user_data_dir)
        }
        "windsurf" => {
            cockpit_core::modules::windsurf_instance::resolve_windsurf_pid(last_pid, user_data_dir)
        }
        "kiro" => cockpit_core::modules::kiro_instance::resolve_kiro_pid(last_pid, user_data_dir),
        "cursor" => {
            cockpit_core::modules::cursor_instance::resolve_cursor_pid(last_pid, user_data_dir)
        }
        "codebuddy" => {
            cockpit_core::modules::process::resolve_codebuddy_pid(last_pid, user_data_dir)
        }
        "codebuddy_cn" => {
            cockpit_core::modules::process::resolve_codebuddy_cn_pid(last_pid, user_data_dir)
        }
        "workbuddy" => {
            cockpit_core::modules::process::resolve_workbuddy_pid(last_pid, user_data_dir)
        }
        "gemini" | "qoder" | "trae" => {
            last_pid.filter(|pid| cockpit_core::modules::process::is_pid_running(*pid))
        }
        _ => None,
    }
}

fn close_instance_process_for_platform(
    platform: &str,
    last_pid: Option<u32>,
    user_data_dir: Option<&str>,
) -> Result<(), String> {
    let dirs = user_data_dir
        .map(|dir| vec![dir.to_string()])
        .unwrap_or_default();
    match platform {
        "antigravity" if !dirs.is_empty() => {
            cockpit_core::modules::process::close_antigravity_instances(&dirs, 20)
        }
        "codex" if !dirs.is_empty() => {
            cockpit_core::modules::process::close_codex_instances(&dirs, 20)
        }
        "github_copilot" if !dirs.is_empty() => {
            cockpit_core::modules::process::close_vscode(&dirs, 20)
        }
        "windsurf" if !dirs.is_empty() => {
            cockpit_core::modules::windsurf_instance::close_windsurf(&dirs, 20)
        }
        "kiro" if !dirs.is_empty() => cockpit_core::modules::kiro_instance::close_kiro(&dirs, 20),
        "cursor" if !dirs.is_empty() => {
            cockpit_core::modules::cursor_instance::close_cursor(&dirs, 20)
        }
        _ => {
            if let Some(pid) = resolve_instance_pid_for_platform(platform, last_pid, user_data_dir)
            {
                cockpit_core::modules::process::close_pid(pid, 20)?;
            }
            Ok(())
        }
    }
}

fn stop_instance_for_platform(platform: &str, params: &Value) -> Result<Value, String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;
    if instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = instance_default_user_data_dir(platform)?;
        let default_dir_str = default_dir.to_string_lossy().to_string();
        let settings = load_default_settings_for_platform(platform)?;
        close_instance_process_for_platform(platform, settings.last_pid, Some(&default_dir_str))?;
        if platform == "codex" {
            block_on_value(
                cockpit_core::modules::codex_local_access::stop_provider_gateways_for_profile(
                    &default_dir,
                ),
            )?;
        }
        let updated = update_default_pid_for_platform(platform, None)?;
        return default_instance_view_value(platform, &updated);
    }

    let instance = find_instance_for_platform(platform, &instance_id)?;
    close_instance_process_for_platform(
        platform,
        instance.last_pid,
        Some(&instance.user_data_dir),
    )?;
    if platform == "codex" {
        block_on_value(
            cockpit_core::modules::codex_local_access::stop_provider_gateways_for_profile(
                Path::new(&instance.user_data_dir),
            ),
        )?;
    }
    let updated = update_instance_pid_for_platform(platform, &instance.id, None)?;
    if platform == "codex" {
        Ok(codex_instance_view_value(updated, false, false))
    } else {
        Ok(generic_instance_view_value(platform, updated))
    }
}

fn open_instance_window_for_platform(platform: &str, params: &Value) -> Result<Value, String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;
    if platform == "gemini" {
        return Err("Gemini CLI instances do not have a managed app window".to_string());
    }

    let (last_pid, user_data_dir, launch_mode) = if instance_id == DEFAULT_INSTANCE_ID {
        let settings = load_default_settings_for_platform(platform)?;
        (settings.last_pid, None, settings.launch_mode)
    } else {
        let instance = find_instance_for_platform(platform, &instance_id)?;
        (
            instance.last_pid,
            Some(instance.user_data_dir),
            instance.launch_mode,
        )
    };

    if platform == "codex" && launch_mode == cockpit_core::models::InstanceLaunchMode::Cli {
        return Err("CLI mode Codex instances do not have a managed app window".to_string());
    }

    match platform {
        "antigravity" => cockpit_core::modules::process::focus_antigravity_instance(
            last_pid,
            user_data_dir.as_deref(),
        ),
        "codex" => {
            cockpit_core::modules::process::focus_codex_instance(last_pid, user_data_dir.as_deref())
        }
        "github_copilot" => cockpit_core::modules::process::focus_vscode_instance(
            last_pid,
            user_data_dir.as_deref(),
        ),
        "windsurf" => cockpit_core::modules::windsurf_instance::focus_windsurf_instance(
            last_pid,
            user_data_dir.as_deref(),
        ),
        "kiro" => cockpit_core::modules::kiro_instance::focus_kiro_instance(
            last_pid,
            user_data_dir.as_deref(),
        ),
        "cursor" => cockpit_core::modules::cursor_instance::focus_cursor_instance(
            last_pid,
            user_data_dir.as_deref(),
        ),
        "codebuddy" | "codebuddy_cn" | "qoder" | "trae" | "workbuddy" => {
            let pid =
                resolve_instance_pid_for_platform(platform, last_pid, user_data_dir.as_deref())
                    .ok_or_else(|| "instance is not running".to_string())?;
            cockpit_core::modules::process::focus_process_pid(pid).map(|_| pid)
        }
        other => Err(format!("unsupported instance platform: {other}")),
    }?;
    Ok(Value::Null)
}

fn close_all_instances_for_platform(platform: &str) -> Result<Value, String> {
    let store = load_instance_store_for_platform(platform)?;
    let default_dir = instance_default_user_data_dir(platform)?;
    let mut target_dirs = vec![default_dir.to_string_lossy().to_string()];
    for instance in &store.instances {
        let dir = instance.user_data_dir.trim();
        if !dir.is_empty() {
            target_dirs.push(dir.to_string());
        }
    }

    match platform {
        "antigravity" => {
            cockpit_core::modules::process::close_antigravity_instances(&target_dirs, 20)?
        }
        "codex" => {
            cockpit_core::modules::process::close_codex_instances(&target_dirs, 20)?;
            block_on_value(
                cockpit_core::modules::codex_local_access::stop_provider_gateways_for_profile(
                    &default_dir,
                ),
            )?;
            for instance in &store.instances {
                let dir = instance.user_data_dir.trim();
                if !dir.is_empty() {
                    block_on_value(
                        cockpit_core::modules::codex_local_access::stop_provider_gateways_for_profile(
                            Path::new(dir),
                        ),
                    )?;
                }
            }
        }
        "github_copilot" => cockpit_core::modules::process::close_vscode(&target_dirs, 20)?,
        "windsurf" => cockpit_core::modules::windsurf_instance::close_windsurf(&target_dirs, 20)?,
        "kiro" => cockpit_core::modules::kiro_instance::close_kiro(&target_dirs, 20)?,
        "cursor" => cockpit_core::modules::cursor_instance::close_cursor(&target_dirs, 20)?,
        "gemini" => {}
        "codebuddy" | "codebuddy_cn" | "qoder" | "trae" | "workbuddy" => {
            let settings = store.default_settings.clone();
            if let Some(pid) = resolve_instance_pid_for_platform(platform, settings.last_pid, None)
            {
                let _ = cockpit_core::modules::process::close_pid(pid, 20);
            }
            for instance in &store.instances {
                if let Some(pid) = resolve_instance_pid_for_platform(
                    platform,
                    instance.last_pid,
                    Some(&instance.user_data_dir),
                ) {
                    let _ = cockpit_core::modules::process::close_pid(pid, 20);
                }
            }
        }
        other => return Err(format!("unsupported instance platform: {other}")),
    }

    clear_all_pids_for_platform(platform)?;
    Ok(Value::Null)
}

fn ensure_launch_path_for_platform(platform: &str) -> Result<(), String> {
    match platform {
        "antigravity" => {
            cockpit_core::modules::process::ensure_antigravity_launch_path_configured()
        }
        "codex" => cockpit_core::modules::process::ensure_codex_launch_path_configured(),
        "github_copilot" => cockpit_core::modules::process::ensure_vscode_launch_path_configured(),
        "windsurf" => {
            cockpit_core::modules::windsurf_instance::ensure_windsurf_launch_path_configured()
        }
        "kiro" => cockpit_core::modules::kiro_instance::ensure_kiro_launch_path_configured(),
        "cursor" => cockpit_core::modules::cursor_instance::ensure_cursor_launch_path_configured(),
        "codebuddy" => cockpit_core::modules::process::ensure_codebuddy_launch_path_configured(),
        "codebuddy_cn" => {
            cockpit_core::modules::process::ensure_codebuddy_cn_launch_path_configured()
        }
        "qoder" => cockpit_core::modules::process::ensure_qoder_launch_path_configured(),
        "trae" => cockpit_core::modules::process::ensure_trae_launch_path_configured(),
        "workbuddy" => cockpit_core::modules::process::ensure_workbuddy_launch_path_configured(),
        "gemini" => Ok(()),
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn default_bind_account_id_for_platform(
    platform: &str,
    settings: &cockpit_core::models::DefaultInstanceSettings,
) -> Result<Option<String>, String> {
    if platform == "antigravity" && settings.follow_local_account {
        return cockpit_core::modules::account::get_current_account()
            .map(|account| account.map(|item| item.id));
    }
    if platform == "codex" && settings.follow_local_account {
        return Ok(cockpit_core::modules::codex_account::get_current_account().map(|item| item.id));
    }
    Ok(settings.bind_account_id.clone())
}

fn trim_optional_id(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|item| !item.is_empty())
}

fn inject_github_copilot_bound_account(
    user_data_dir: &str,
    account_id: &str,
) -> Result<(), String> {
    let account = cockpit_core::modules::github_copilot_account::load_account(account_id)
        .ok_or_else(|| format!("bound account not found: {account_id}"))?;
    cockpit_core::modules::process::close_vscode(&[user_data_dir.to_string()], 20)?;
    let github_id = account.github_id.to_string();
    cockpit_core::modules::vscode_inject::inject_copilot_token_for_user_data_dir(
        user_data_dir,
        &account.github_login,
        &account.github_access_token,
        Some(&github_id),
    )
    .map(|_| ())
}

fn parse_codex_provider_gateway_bind_account_id(account_id: &str) -> Option<String> {
    account_id
        .trim()
        .strip_prefix("provider_gateway_")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn inject_bound_account_for_instance_start(
    platform: &str,
    user_data_dir: &str,
    bind_account_id: Option<&str>,
) -> Result<(), String> {
    let Some(account_id) = trim_optional_id(bind_account_id) else {
        return Ok(());
    };
    let profile_dir = Path::new(user_data_dir);
    match platform {
        "antigravity" => {
            block_on(cockpit_core::modules::account::prepare_account_for_injection(account_id))?;
            cockpit_core::modules::instance::inject_account_to_profile(profile_dir, account_id)
        }
        "codex" => {
            if cockpit_core::modules::codex_instance::is_api_service_bind_account_id(account_id) {
                return block_on(
                    cockpit_core::modules::codex_local_access::activate_local_access_for_dir(
                        profile_dir,
                    ),
                )
                .map(|_| ());
            }
            if let Some(provider_gateway_account_id) =
                parse_codex_provider_gateway_bind_account_id(account_id)
            {
                return block_on(
                    cockpit_core::modules::codex_local_access::activate_provider_gateway_for_dir(
                        profile_dir,
                        &provider_gateway_account_id,
                    ),
                )
                .map(|_| ());
            }
            cockpit_core::modules::codex_local_access::cleanup_provider_gateway_profile_model_overrides(
                profile_dir,
            )?;
            block_on(
                cockpit_core::modules::codex_instance::inject_account_to_profile(
                    profile_dir,
                    account_id,
                ),
            )
        }
        "github_copilot" => inject_github_copilot_bound_account(user_data_dir, account_id),
        "windsurf" => {
            let account = cockpit_core::modules::windsurf_account::load_account(account_id)
                .ok_or_else(|| format!("bound account not found: {account_id}"))?;
            let is_devin = account
                .devin_auth1_token
                .as_deref()
                .map(|token| token.starts_with("auth1_"))
                .unwrap_or(false);
            if is_devin {
                let _ = block_on(
                    cockpit_core::modules::windsurf_account::refresh_account_token(account_id),
                );
            }
            cockpit_core::modules::windsurf_instance::inject_account_to_profile(
                profile_dir,
                account_id,
            )
        }
        "kiro" => {
            cockpit_core::modules::kiro_instance::inject_account_to_profile(profile_dir, account_id)
        }
        "cursor" => {
            cockpit_core::modules::cursor_instance::close_cursor(&[user_data_dir.to_string()], 20)?;
            cockpit_core::modules::cursor_instance::inject_account_to_profile(
                profile_dir,
                account_id,
            )
        }
        "gemini" => cockpit_core::modules::gemini_account::inject_to_gemini_home(
            account_id,
            Some(profile_dir),
        ),
        "qoder" => cockpit_core::modules::qoder_account::inject_to_qoder_for_user_data_dir(
            user_data_dir,
            account_id,
        ),
        "trae" => {
            block_on(cockpit_core::modules::trae_account::refresh_account_async(
                account_id,
            ))?;
            let storage_path =
                cockpit_core::modules::trae_instance::build_storage_json_path(user_data_dir);
            cockpit_core::modules::trae_account::inject_to_trae_at_path(
                storage_path.as_path(),
                account_id,
            )
        }
        "codebuddy" => cockpit_core::modules::codebuddy_account::inject_to_codebuddy_user_data_dir(
            user_data_dir,
            account_id,
        ),
        "codebuddy_cn" => {
            cockpit_core::modules::codebuddy_cn_account::inject_to_codebuddy_cn_user_data_dir(
                user_data_dir,
                account_id,
            )
        }
        "workbuddy" => {
            cockpit_core::modules::workbuddy_account::sync_account_to_default_client(account_id)
        }
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn start_process_for_platform(
    platform: &str,
    user_data_dir: Option<&str>,
    extra_args: &[String],
) -> Result<u32, String> {
    match (platform, user_data_dir) {
        ("antigravity", Some(dir)) => {
            cockpit_core::modules::process::start_antigravity_with_args(dir, extra_args)
        }
        ("antigravity", None) => {
            cockpit_core::modules::process::start_antigravity_with_args("", extra_args)
        }
        ("codex", Some(dir)) => cockpit_core::modules::process::start_codex_with_args(dir, extra_args),
        ("codex", None) => cockpit_core::modules::process::start_codex_default(extra_args),
        ("github_copilot", Some(dir)) => {
            cockpit_core::modules::process::start_vscode_with_args_with_new_window(
                dir, extra_args, true,
            )
        }
        ("github_copilot", None) => {
            cockpit_core::modules::process::start_vscode_default_with_args_with_new_window(
                extra_args, true,
            )
        }
        ("windsurf", Some(dir)) => {
            cockpit_core::modules::windsurf_instance::start_windsurf_with_args_with_new_window(
                dir, extra_args, true,
            )
        }
        ("windsurf", None) => {
            cockpit_core::modules::windsurf_instance::start_windsurf_default_with_args_with_new_window(
                extra_args, true,
            )
        }
        ("kiro", Some(dir)) => {
            cockpit_core::modules::kiro_instance::start_kiro_with_args_with_new_window(
                dir, extra_args, true,
            )
        }
        ("kiro", None) => {
            cockpit_core::modules::kiro_instance::start_kiro_default_with_args_with_new_window(
                extra_args, true,
            )
        }
        ("cursor", Some(dir)) => {
            cockpit_core::modules::cursor_instance::start_cursor_with_args_with_new_window(
                dir, extra_args, true,
            )
        }
        ("cursor", None) => {
            cockpit_core::modules::cursor_instance::start_cursor_default_with_args_with_new_window(
                extra_args, true,
            )
        }
        ("qoder", Some(dir)) => {
            cockpit_core::modules::process::start_qoder_with_args_with_new_window(
                dir, extra_args, true,
            )
        }
        ("qoder", None) => {
            cockpit_core::modules::process::start_qoder_default_with_args_with_new_window(
                extra_args, true,
            )
        }
        ("trae", Some(dir)) => {
            cockpit_core::modules::process::start_trae_with_args_with_new_window(
                dir, extra_args, true,
            )
        }
        ("trae", None) => {
            cockpit_core::modules::process::start_trae_default_with_args_with_new_window(
                extra_args, true,
            )
        }
        ("codebuddy", Some(dir)) => {
            cockpit_core::modules::process::start_codebuddy_with_args_with_new_window(
                dir, extra_args, true,
            )
        }
        ("codebuddy", None) => {
            cockpit_core::modules::process::start_codebuddy_default_with_args_with_new_window(
                extra_args, true,
            )
        }
        ("codebuddy_cn", Some(dir)) => {
            cockpit_core::modules::process::start_codebuddy_cn_with_args_with_new_window(
                dir, extra_args, true,
            )
        }
        ("codebuddy_cn", None) => {
            cockpit_core::modules::process::start_codebuddy_cn_default_with_args_with_new_window(
                extra_args, true,
            )
        }
        ("workbuddy", Some(dir)) => {
            cockpit_core::modules::process::start_workbuddy_with_args_with_new_window(
                dir, extra_args, true,
            )
        }
        ("workbuddy", None) => {
            cockpit_core::modules::process::start_workbuddy_default_with_args_with_new_window(
                extra_args, true,
            )
        }
        ("gemini", _) => Err("Gemini CLI instances are launched from terminal commands".to_string()),
        (other, _) => Err(format!("unsupported instance platform: {other}")),
    }
}

fn start_instance_for_platform(platform: &str, params: &Value) -> Result<Value, String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;

    if platform == "gemini" {
        if instance_id == DEFAULT_INSTANCE_ID {
            let default_dir =
                cockpit_core::modules::gemini_instance::get_default_gemini_cli_home_root()?;
            let settings = cockpit_core::modules::gemini_instance::load_default_settings()?;
            if let Some(account_id) = settings.bind_account_id.as_deref() {
                inject_bound_account_for_instance_start(
                    platform,
                    &default_dir.to_string_lossy(),
                    Some(account_id),
                )?;
            }
            let updated = cockpit_core::modules::gemini_instance::update_default_pid(None)?;
            return default_instance_view_value(platform, &updated);
        }
        let instance = find_instance_for_platform(platform, &instance_id)?;
        if let Some(account_id) = instance.bind_account_id.as_deref() {
            inject_bound_account_for_instance_start(
                platform,
                &instance.user_data_dir,
                Some(account_id),
            )?;
        }
        let updated =
            cockpit_core::modules::gemini_instance::update_instance_last_launched(&instance.id)?;
        return Ok(generic_instance_view_value(platform, updated));
    }

    if instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = instance_default_user_data_dir(platform)?;
        let default_dir_str = default_dir.to_string_lossy().to_string();
        let settings = load_default_settings_for_platform(platform)?;
        let bind_account_id = default_bind_account_id_for_platform(platform, &settings)?;
        close_instance_process_for_platform(platform, settings.last_pid, Some(&default_dir_str))?;
        let _ = update_default_pid_for_platform(platform, None)?;

        if platform == "codex" {
            block_on_value(
                cockpit_core::modules::codex_local_access::stop_provider_gateways_for_profile(
                    &default_dir,
                ),
            )?;
            cockpit_core::modules::codex_speed::write_app_speed_for_dir(
                &default_dir,
                settings.app_speed.clone(),
            )?;
        }

        if let Some(ref account_id) = bind_account_id {
            inject_bound_account_for_instance_start(platform, &default_dir_str, Some(account_id))?;
        } else if platform == "codex" {
            cockpit_core::modules::codex_local_access::cleanup_provider_gateway_profile_model_overrides(
                &default_dir,
            )?;
        }

        if platform == "codex"
            && settings.launch_mode == cockpit_core::models::InstanceLaunchMode::Cli
        {
            let command =
                build_codex_launch_command(&resolve_codex_launch_context(DEFAULT_INSTANCE_ID)?)?;
            let _ = command;
            let updated = update_default_pid_for_platform(platform, None)?;
            return default_instance_view_value(platform, &updated);
        }

        ensure_launch_path_for_platform(platform)?;
        let extra_args = cockpit_core::modules::process::parse_extra_args(&settings.extra_args);
        let pid = start_process_for_platform(platform, None, &extra_args)?;
        let updated = update_default_pid_for_platform(platform, Some(pid))?;
        return default_instance_view_value(platform, &updated);
    }

    let instance = find_instance_for_platform(platform, &instance_id)?;
    close_instance_process_for_platform(
        platform,
        instance.last_pid,
        Some(&instance.user_data_dir),
    )?;
    let _ = update_instance_pid_for_platform(platform, &instance.id, None)?;

    if platform == "codex" {
        let instance_dir = Path::new(&instance.user_data_dir);
        cockpit_core::modules::codex_instance::ensure_instance_shared_skills(instance_dir)?;
        block_on_value(
            cockpit_core::modules::codex_local_access::stop_provider_gateways_for_profile(
                instance_dir,
            ),
        )?;
        cockpit_core::modules::codex_speed::write_app_speed_for_dir(
            instance_dir,
            instance.app_speed.clone(),
        )?;
    }

    if let Some(account_id) = instance.bind_account_id.as_deref() {
        inject_bound_account_for_instance_start(
            platform,
            &instance.user_data_dir,
            Some(account_id),
        )?;
    } else if platform == "codex" {
        cockpit_core::modules::codex_local_access::cleanup_provider_gateway_profile_model_overrides(
            Path::new(&instance.user_data_dir),
        )?;
    }

    if platform == "codex" && instance.launch_mode == cockpit_core::models::InstanceLaunchMode::Cli
    {
        let command = build_codex_launch_command(&resolve_codex_launch_context(&instance.id)?)?;
        let _ = command;
        let updated =
            cockpit_core::modules::codex_instance::update_instance_after_cli_prepare(&instance.id)?;
        return Ok(codex_instance_view_value(updated, false, false));
    }

    ensure_launch_path_for_platform(platform)?;
    let extra_args = cockpit_core::modules::process::parse_extra_args(&instance.extra_args);
    let pid = start_process_for_platform(platform, Some(&instance.user_data_dir), &extra_args)?;
    let updated = match platform {
        "antigravity" => {
            cockpit_core::modules::instance::update_instance_after_start(&instance.id, pid)
        }
        "codex" => {
            cockpit_core::modules::codex_instance::update_instance_after_start(&instance.id, pid)
        }
        "github_copilot" => {
            cockpit_core::modules::github_copilot_instance::update_instance_after_start(
                &instance.id,
                pid,
            )
        }
        "windsurf" => {
            cockpit_core::modules::windsurf_instance::update_instance_after_start(&instance.id, pid)
        }
        "kiro" => {
            cockpit_core::modules::kiro_instance::update_instance_after_start(&instance.id, pid)
        }
        "cursor" => {
            cockpit_core::modules::cursor_instance::update_instance_after_start(&instance.id, pid)
        }
        "codebuddy" => cockpit_core::modules::codebuddy_instance::update_instance_after_start(
            &instance.id,
            pid,
        ),
        "codebuddy_cn" => {
            cockpit_core::modules::codebuddy_cn_instance::update_instance_after_start(
                &instance.id,
                pid,
            )
        }
        "qoder" => {
            cockpit_core::modules::qoder_instance::update_instance_after_start(&instance.id, pid)
        }
        "trae" => {
            cockpit_core::modules::trae_instance::update_instance_after_start(&instance.id, pid)
        }
        "workbuddy" => cockpit_core::modules::workbuddy_instance::update_instance_after_start(
            &instance.id,
            pid,
        ),
        other => return Err(format!("unsupported instance platform: {other}")),
    }?;

    if platform == "codex" {
        Ok(codex_instance_view_value(updated, false, false))
    } else {
        Ok(generic_instance_view_value(platform, updated))
    }
}

#[cfg(not(target_os = "windows"))]
fn posix_shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    let needs_quote = value.chars().any(|ch| {
        ch.is_whitespace()
            || matches!(
                ch,
                '\'' | '"' | '$' | '`' | '\\' | '&' | '|' | ';' | '<' | '>' | '(' | ')'
            )
    });
    if !needs_quote {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "windows")]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn windows_cmd_quote(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quote = value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '^' | '&' | '|' | '<' | '>' | '%'));
    if !needs_quote {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('"', "\\\""))
}

struct ServiceLaunchContext {
    user_data_dir: String,
    working_dir: Option<String>,
    extra_args: String,
    use_home_env: bool,
}

fn resolve_codex_launch_context(instance_id: &str) -> Result<ServiceLaunchContext, String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        let default_settings = cockpit_core::modules::codex_instance::load_default_settings()?;
        if default_settings.launch_mode != cockpit_core::models::InstanceLaunchMode::Cli {
            return Err("current Codex instance is not configured for CLI launch".to_string());
        }
        let default_dir = cockpit_core::modules::codex_instance::get_default_codex_home()?;
        return Ok(ServiceLaunchContext {
            user_data_dir: default_dir.to_string_lossy().to_string(),
            working_dir: None,
            extra_args: default_settings.extra_args,
            use_home_env: true,
        });
    }

    let instance = find_instance_for_platform("codex", instance_id)?;
    if instance.launch_mode != cockpit_core::models::InstanceLaunchMode::Cli {
        return Err("current Codex instance is not configured for CLI launch".to_string());
    }
    Ok(ServiceLaunchContext {
        user_data_dir: instance.user_data_dir,
        working_dir: instance.working_dir,
        extra_args: instance.extra_args,
        use_home_env: true,
    })
}

fn resolve_gemini_launch_context(instance_id: &str) -> Result<ServiceLaunchContext, String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        let default_dir =
            cockpit_core::modules::gemini_instance::get_default_gemini_cli_home_root()?;
        let default_settings = cockpit_core::modules::gemini_instance::load_default_settings()?;
        return Ok(ServiceLaunchContext {
            user_data_dir: default_dir.to_string_lossy().to_string(),
            working_dir: default_settings.working_dir,
            extra_args: default_settings.extra_args,
            use_home_env: false,
        });
    }

    let instance = find_instance_for_platform("gemini", instance_id)?;
    Ok(ServiceLaunchContext {
        user_data_dir: instance.user_data_dir,
        working_dir: instance.working_dir,
        extra_args: instance.extra_args,
        use_home_env: true,
    })
}

fn build_codex_launch_command(context: &ServiceLaunchContext) -> Result<String, String> {
    cockpit_core::modules::codex_config_format::sanitize_codex_config_toml_file(
        &Path::new(&context.user_data_dir).join("config.toml"),
    )
    .map(|_| ())?;
    let runtime = cockpit_core::modules::codex_wakeup::resolve_cli_runtime()?;
    let parsed_args = cockpit_core::modules::process::parse_extra_args(&context.extra_args);

    #[cfg(not(target_os = "windows"))]
    {
        let mut command_parts = Vec::new();
        if let Some(ref dir) = context.working_dir {
            if !dir.trim().is_empty() {
                command_parts.push(format!("cd {}", posix_shell_quote(dir)));
            }
        }
        let mut codex_cmd = format!("CODEX_HOME={} ", posix_shell_quote(&context.user_data_dir));
        if let Some(node_path) = runtime.node_path.as_deref() {
            codex_cmd.push_str(&posix_shell_quote(node_path));
            codex_cmd.push(' ');
        }
        codex_cmd.push_str(&posix_shell_quote(&runtime.binary_path));
        for arg in parsed_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                codex_cmd.push(' ');
                codex_cmd.push_str(&posix_shell_quote(trimmed));
            }
        }
        command_parts.push(codex_cmd);
        return Ok(command_parts.join(" && "));
    }

    #[cfg(target_os = "windows")]
    {
        let mut command_parts = vec![format!(
            "$env:CODEX_HOME={}",
            powershell_quote(&context.user_data_dir)
        )];
        if let Some(ref dir) = context.working_dir {
            if !dir.trim().is_empty() {
                command_parts.push(format!(
                    "Set-Location -LiteralPath {}",
                    powershell_quote(dir)
                ));
            }
        }
        let mut codex_cmd = String::from("& ");
        if let Some(node_path) = runtime.node_path.as_deref() {
            codex_cmd.push_str(&powershell_quote(node_path));
            codex_cmd.push(' ');
        }
        codex_cmd.push_str(&powershell_quote(&runtime.binary_path));
        for arg in parsed_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                codex_cmd.push(' ');
                codex_cmd.push_str(&powershell_quote(trimmed));
            }
        }
        command_parts.push(codex_cmd);
        return Ok(command_parts.join("; "));
    }

    #[allow(unreachable_code)]
    Err("unsupported platform for Codex CLI launch command".to_string())
}

fn build_gemini_launch_command(context: &ServiceLaunchContext) -> String {
    let parsed_args = cockpit_core::modules::process::parse_extra_args(&context.extra_args);
    let mut command_parts = Vec::new();
    if let Some(ref dir) = context.working_dir {
        if !dir.trim().is_empty() {
            #[cfg(target_os = "windows")]
            command_parts.push(format!("cd /d \"{}\"", dir.replace('"', "\"\"")));
            #[cfg(not(target_os = "windows"))]
            command_parts.push(format!("cd {}", posix_shell_quote(dir)));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if context.use_home_env {
            command_parts.push(format!(
                "set \"GEMINI_CLI_HOME={}\"",
                context.user_data_dir.replace('"', "\"\"")
            ));
        }
        let mut gemini_cmd = "gemini".to_string();
        for arg in parsed_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                gemini_cmd.push(' ');
                gemini_cmd.push_str(&windows_cmd_quote(trimmed));
            }
        }
        command_parts.push(gemini_cmd);
        return command_parts.join(" && ");
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut gemini_cmd = if context.use_home_env {
            format!(
                "GEMINI_CLI_HOME={} gemini",
                posix_shell_quote(&context.user_data_dir)
            )
        } else {
            "gemini".to_string()
        };
        for arg in parsed_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                gemini_cmd.push(' ');
                gemini_cmd.push_str(&posix_shell_quote(trimmed));
            }
        }
        command_parts.push(gemini_cmd);
        command_parts.join(" && ")
    }
}

fn get_instance_launch_command_value(platform: &str, params: &Value) -> Result<Value, String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;
    let (context, command) = match platform {
        "codex" => {
            let context = resolve_codex_launch_context(&instance_id)?;
            let command = build_codex_launch_command(&context)?;
            (context, command)
        }
        "gemini" => {
            let context = resolve_gemini_launch_context(&instance_id)?;
            let command = build_gemini_launch_command(&context);
            (context, command)
        }
        other => {
            return Err(format!(
                "launch command is not supported for {other} instances"
            ))
        }
    };
    Ok(json!({
        "instanceId": instance_id,
        "userDataDir": context.user_data_dir,
        "launchCommand": command
    }))
}

fn execute_terminal_command(
    command: &str,
    terminal: Option<String>,
    label: &str,
) -> Result<String, String> {
    let terminal = terminal
        .unwrap_or_else(|| cockpit_core::modules::config::get_user_config().default_terminal)
        .trim()
        .to_string();

    #[cfg(target_os = "windows")]
    {
        let mut cmd = if terminal == "pwsh" {
            let mut command_process = Command::new("pwsh");
            command_process.args(["-NoExit", "-Command", command]);
            command_process
        } else if terminal == "wt" {
            let mut command_process = Command::new("wt");
            command_process.args(["powershell", "-NoExit", "-Command", command]);
            command_process
        } else if terminal == "cmd" {
            let mut command_process = Command::new("cmd");
            command_process.args([
                "/C",
                "start",
                "",
                "powershell",
                "-NoExit",
                "-Command",
                command,
            ]);
            command_process
        } else if terminal == "PowerShell"
            || terminal == "powershell"
            || terminal.is_empty()
            || terminal == "system"
        {
            let mut command_process = Command::new("powershell");
            command_process.args(["-NoExit", "-Command", command]);
            command_process
        } else {
            let mut command_process = Command::new(&terminal);
            command_process.args(["-NoExit", "-Command", command]);
            command_process
        };
        cmd.spawn()
            .map_err(|err| format!("open terminal on service host failed: {err}"))?;
        return Ok(format!(
            "started {label} command in a service-host terminal"
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let is_iterm = terminal.to_lowercase().contains("iterm");
        let is_terminal_app = terminal == "system" || terminal.is_empty() || terminal == "Terminal";
        let app_name = if is_terminal_app {
            "Terminal"
        } else {
            &terminal
        };
        let escaped = command
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        let script = if is_iterm {
            format!(
                "tell application \"iTerm\"
                    activate
                    if not (exists window 1) then
                        create window with default profile
                        tell current session of current window
                            write text \"{}\"
                        end tell
                    else
                        tell current window
                            create tab with default profile
                            tell current session
                                write text \"{}\"
                            end tell
                        end tell
                    end if
                end tell",
                escaped, escaped
            )
        } else if is_terminal_app {
            format!(
                "tell application \"Terminal\"
                    activate
                    do script \"{}\"
                end tell",
                escaped
            )
        } else {
            return Err(format!("unsupported terminal on service host: {terminal}"));
        };
        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|err| format!("open terminal on service host failed ({app_name}): {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "terminal execution failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        return Ok(format!(
            "started {label} command in {app_name} on the service host"
        ));
    }

    #[cfg(target_os = "linux")]
    {
        let shell_command = format!("{}; exec bash", command);
        let mut cmd = if terminal == "system" || terminal.is_empty() {
            Command::new("x-terminal-emulator")
        } else {
            Command::new(&terminal)
        };
        cmd.args(["-e", "bash", "-lc", &shell_command])
            .spawn()
            .or_else(|_| {
                if terminal == "system" || terminal.is_empty() {
                    Command::new("gnome-terminal")
                        .args(["--", "bash", "-lc", &shell_command])
                        .spawn()
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "terminal not found",
                    ))
                }
            })
            .or_else(|_| Command::new("sh").args(["-lc", command]).spawn())
            .map_err(|err| format!("execute {label} command on service host failed: {err}"))?;
        return Ok(format!("started {label} command on the service host"));
    }

    #[allow(unreachable_code)]
    Err("unsupported service host OS".to_string())
}

fn execute_instance_launch_command_value(platform: &str, params: &Value) -> Result<Value, String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;
    let terminal = param_optional_string(params, &["terminal"])?;
    let command = match platform {
        "codex" => build_codex_launch_command(&resolve_codex_launch_context(&instance_id)?)?,
        "gemini" => build_gemini_launch_command(&resolve_gemini_launch_context(&instance_id)?),
        other => {
            return Err(format!(
                "launch command is not supported for {other} instances"
            ))
        }
    };
    to_value_result(Ok::<_, String>(execute_terminal_command(
        &command, terminal, platform,
    )?))
}

fn normalize_instance_platform(platform: &str) -> Result<&'static str, String> {
    match platform {
        "antigravity" => Ok("antigravity"),
        "codex" => Ok("codex"),
        "github-copilot" | "github_copilot" => Ok("github_copilot"),
        "windsurf" => Ok("windsurf"),
        "kiro" => Ok("kiro"),
        "cursor" => Ok("cursor"),
        "gemini" => Ok("gemini"),
        "codebuddy" => Ok("codebuddy"),
        "codebuddy-cn" | "codebuddy_cn" => Ok("codebuddy_cn"),
        "qoder" => Ok("qoder"),
        "trae" => Ok("trae"),
        "workbuddy" => Ok("workbuddy"),
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn instance_default_user_data_dir(platform: &str) -> Result<PathBuf, String> {
    match platform {
        "antigravity" => cockpit_core::modules::instance::get_default_user_data_dir(),
        "codex" => cockpit_core::modules::codex_instance::get_default_codex_home(),
        "github_copilot" => {
            cockpit_core::modules::github_copilot_instance::get_default_vscode_user_data_dir()
        }
        "windsurf" => {
            cockpit_core::modules::windsurf_instance::get_default_windsurf_user_data_dir()
        }
        "kiro" => cockpit_core::modules::kiro_instance::get_default_kiro_user_data_dir(),
        "cursor" => cockpit_core::modules::cursor_instance::get_default_cursor_user_data_dir(),
        "gemini" => cockpit_core::modules::gemini_instance::get_default_gemini_cli_home_root(),
        "codebuddy" => {
            cockpit_core::modules::codebuddy_instance::get_default_codebuddy_user_data_dir()
        }
        "codebuddy_cn" => {
            cockpit_core::modules::codebuddy_cn_instance::get_default_codebuddy_cn_user_data_dir()
        }
        "qoder" => cockpit_core::modules::qoder_instance::get_default_qoder_user_data_dir(),
        "trae" => cockpit_core::modules::trae_instance::get_default_trae_user_data_dir(),
        "workbuddy" => {
            cockpit_core::modules::workbuddy_instance::get_default_workbuddy_user_data_dir()
        }
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn instance_defaults_value(platform: &str) -> Result<Value, String> {
    match platform {
        "antigravity" => to_value_result(cockpit_core::modules::instance::get_instance_defaults()),
        "codex" => to_value_result(cockpit_core::modules::codex_instance::get_instance_defaults()),
        "github_copilot" => {
            to_value_result(cockpit_core::modules::github_copilot_instance::get_instance_defaults())
        }
        "windsurf" => {
            to_value_result(cockpit_core::modules::windsurf_instance::get_instance_defaults())
        }
        "kiro" => to_value_result(cockpit_core::modules::kiro_instance::get_instance_defaults()),
        "cursor" => {
            to_value_result(cockpit_core::modules::cursor_instance::get_instance_defaults())
        }
        "gemini" => {
            to_value_result(cockpit_core::modules::gemini_instance::get_instance_defaults())
        }
        "codebuddy" => {
            to_value_result(cockpit_core::modules::codebuddy_instance::get_instance_defaults())
        }
        "codebuddy_cn" => {
            to_value_result(cockpit_core::modules::codebuddy_cn_instance::get_instance_defaults())
        }
        "qoder" => to_value_result(cockpit_core::modules::qoder_instance::get_instance_defaults()),
        "trae" => to_value_result(cockpit_core::modules::trae_instance::get_instance_defaults()),
        "workbuddy" => {
            to_value_result(cockpit_core::modules::workbuddy_instance::get_instance_defaults())
        }
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn load_instance_store_for_platform(
    platform: &str,
) -> Result<cockpit_core::models::InstanceStore, String> {
    match platform {
        "antigravity" => cockpit_core::modules::instance::load_instance_store(),
        "codex" => cockpit_core::modules::codex_instance::load_instance_store(),
        "github_copilot" => cockpit_core::modules::github_copilot_instance::load_instance_store(),
        "windsurf" => cockpit_core::modules::windsurf_instance::load_instance_store(),
        "kiro" => cockpit_core::modules::kiro_instance::load_instance_store(),
        "cursor" => cockpit_core::modules::cursor_instance::load_instance_store(),
        "gemini" => cockpit_core::modules::gemini_instance::load_instance_store(),
        "codebuddy" => cockpit_core::modules::codebuddy_instance::load_instance_store(),
        "codebuddy_cn" => cockpit_core::modules::codebuddy_cn_instance::load_instance_store(),
        "qoder" => cockpit_core::modules::qoder_instance::load_instance_store(),
        "trae" => cockpit_core::modules::trae_instance::load_instance_store(),
        "workbuddy" => cockpit_core::modules::workbuddy_instance::load_instance_store(),
        other => Err(format!("unsupported instance platform: {other}")),
    }
}

fn is_instance_profile_initialized(platform: &str, user_data_dir: &str) -> bool {
    if platform == "gemini" {
        return cockpit_core::modules::gemini_instance::is_profile_initialized(Path::new(
            user_data_dir,
        ));
    }
    cockpit_core::modules::instance::is_profile_initialized(Path::new(user_data_dir))
}

fn instance_running(last_pid: Option<u32>) -> bool {
    last_pid
        .map(cockpit_core::modules::process::is_pid_running)
        .unwrap_or(false)
}

fn generic_instance_view_value(
    platform: &str,
    profile: cockpit_core::models::InstanceProfile,
) -> Value {
    let running = instance_running(profile.last_pid);
    let initialized = is_instance_profile_initialized(platform, &profile.user_data_dir);
    serde_json::to_value(cockpit_core::models::InstanceProfileView::from_profile(
        profile,
        running,
        initialized,
    ))
    .unwrap_or(Value::Null)
}

fn codex_instance_view_value(
    profile: cockpit_core::models::InstanceProfile,
    follow_local_account: bool,
    auto_sync_threads: bool,
) -> Value {
    let running = instance_running(profile.last_pid);
    let initialized = is_instance_profile_initialized("codex", &profile.user_data_dir);
    json!({
        "id": profile.id,
        "name": profile.name,
        "userDataDir": profile.user_data_dir,
        "workingDir": profile.working_dir,
        "extraArgs": profile.extra_args,
        "bindAccountId": profile.bind_account_id,
        "launchMode": profile.launch_mode,
        "appSpeed": profile.app_speed,
        "createdAt": profile.created_at,
        "lastLaunchedAt": profile.last_launched_at,
        "lastPid": profile.last_pid,
        "running": running,
        "initialized": initialized,
        "isDefault": false,
        "followLocalAccount": follow_local_account,
        "autoSyncThreads": auto_sync_threads,
        "codexLaunchCredentialChange": null
    })
}

fn codex_instance_result_to_view(value: Value) -> Value {
    serde_json::from_value::<cockpit_core::models::InstanceProfile>(value.clone())
        .map(|profile| codex_instance_view_value(profile, false, false))
        .unwrap_or(value)
}

fn default_instance_view_value(
    platform: &str,
    settings: &cockpit_core::models::DefaultInstanceSettings,
) -> Result<Value, String> {
    let default_dir = instance_default_user_data_dir(platform)?;
    let default_dir_str = default_dir.to_string_lossy().to_string();
    if platform == "codex" {
        return Ok(json!({
            "id": DEFAULT_INSTANCE_ID,
            "name": "",
            "userDataDir": default_dir_str,
            "workingDir": settings.working_dir,
            "extraArgs": settings.extra_args,
            "bindAccountId": settings.bind_account_id,
            "launchMode": settings.launch_mode,
            "appSpeed": settings.app_speed,
            "createdAt": 0,
            "lastLaunchedAt": null,
            "lastPid": settings.last_pid,
            "running": instance_running(settings.last_pid),
            "initialized": is_instance_profile_initialized(platform, &default_dir_str),
            "isDefault": true,
            "followLocalAccount": settings.follow_local_account,
            "autoSyncThreads": settings.auto_sync_threads,
            "codexLaunchCredentialChange": null
        }));
    }
    Ok(
        serde_json::to_value(cockpit_core::models::InstanceProfileView {
            id: DEFAULT_INSTANCE_ID.to_string(),
            name: String::new(),
            user_data_dir: default_dir_str.clone(),
            working_dir: settings.working_dir.clone(),
            extra_args: settings.extra_args.clone(),
            bind_account_id: settings.bind_account_id.clone(),
            created_at: 0,
            last_launched_at: None,
            last_pid: settings.last_pid,
            running: instance_running(settings.last_pid),
            initialized: is_instance_profile_initialized(platform, &default_dir_str),
            is_default: true,
            follow_local_account: platform == "antigravity" && settings.follow_local_account,
        })
        .unwrap_or(Value::Null),
    )
}

fn list_instances_value(platform: &str) -> Result<Value, String> {
    let store = load_instance_store_for_platform(platform)?;
    let mut values: Vec<Value> = store
        .instances
        .into_iter()
        .map(|profile| {
            if platform == "codex" {
                codex_instance_view_value(profile, false, false)
            } else {
                generic_instance_view_value(platform, profile)
            }
        })
        .collect();
    values.push(default_instance_view_value(
        platform,
        &store.default_settings,
    )?);
    Ok(Value::Array(values))
}

fn create_generic_instance_value(platform: &str, params: &Value) -> Result<Value, String> {
    let payload = cockpit_core::modules::instance_store::CreateInstanceParams {
        name: param_string(params, &["name"])?,
        user_data_dir: param_string(params, &["userDataDir", "user_data_dir"])?,
        working_dir: param_optional_string(params, &["workingDir", "working_dir"])?,
        extra_args: param_optional_string(params, &["extraArgs", "extra_args"])?
            .unwrap_or_default(),
        bind_account_id: param_optional_string(params, &["bindAccountId", "bind_account_id"])?,
        copy_source_instance_id: param_optional_string(
            params,
            &["copySourceInstanceId", "copy_source_instance_id"],
        )?,
        init_mode: param_optional_string(params, &["initMode", "init_mode"])?,
    };
    let created = match platform {
        "antigravity" => cockpit_core::modules::instance::create_instance(payload),
        "github_copilot" => {
            cockpit_core::modules::github_copilot_instance::create_instance(payload)
        }
        "windsurf" => cockpit_core::modules::windsurf_instance::create_instance(payload),
        "kiro" => cockpit_core::modules::kiro_instance::create_instance(payload),
        "cursor" => cockpit_core::modules::cursor_instance::create_instance(payload),
        "gemini" => cockpit_core::modules::gemini_instance::create_instance(payload),
        "codebuddy" => cockpit_core::modules::codebuddy_instance::create_instance(payload),
        "codebuddy_cn" => cockpit_core::modules::codebuddy_cn_instance::create_instance(payload),
        "qoder" => cockpit_core::modules::qoder_instance::create_instance(payload),
        "trae" => cockpit_core::modules::trae_instance::create_instance(payload),
        "workbuddy" => cockpit_core::modules::workbuddy_instance::create_instance(payload),
        other => return Err(format!("unsupported instance platform: {other}")),
    }?;
    Ok(generic_instance_view_value(platform, created))
}

fn update_default_instance_value(platform: &str, params: &Value) -> Result<Value, String> {
    let bind_account_id =
        param_nullable_string_presence(params, &["bindAccountId", "bind_account_id"])?;
    let _working_dir =
        param_nullable_string_presence(params, &["workingDir", "working_dir"])?.flatten();
    let extra_args = param_optional_string(params, &["extraArgs", "extra_args"])?;
    let follow_local_account =
        param_optional_bool(params, &["followLocalAccount", "follow_local_account"])?;
    let updated = match platform {
        "antigravity" => cockpit_core::modules::instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        "codex" => {
            let launch_mode = param_optional_launch_mode(params)?;
            let mut settings = cockpit_core::modules::codex_instance::update_default_settings(
                bind_account_id,
                extra_args,
                follow_local_account,
                launch_mode,
            )?;
            if let Some(app_speed) = param_optional_json::<
                cockpit_core::models::codex::CodexAppSpeed,
            >(params, &["appSpeed", "app_speed"])?
            {
                settings =
                    cockpit_core::modules::codex_instance::update_default_app_speed(app_speed)?;
            }
            Ok(settings)
        }
        "gemini" => cockpit_core::modules::gemini_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        "github_copilot" => {
            cockpit_core::modules::github_copilot_instance::update_default_settings(
                bind_account_id,
                extra_args,
                follow_local_account,
            )
        }
        "windsurf" => cockpit_core::modules::windsurf_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        "kiro" => cockpit_core::modules::kiro_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        "cursor" => cockpit_core::modules::cursor_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        "codebuddy" => cockpit_core::modules::codebuddy_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        "codebuddy_cn" => cockpit_core::modules::codebuddy_cn_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        "qoder" => cockpit_core::modules::qoder_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        "trae" => cockpit_core::modules::trae_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        "workbuddy" => cockpit_core::modules::workbuddy_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        other => return Err(format!("unsupported instance platform: {other}")),
    }?;
    default_instance_view_value(platform, &updated)
}

fn update_generic_instance_value(platform: &str, params: &Value) -> Result<Value, String> {
    let payload = cockpit_core::modules::instance_store::UpdateInstanceParams {
        instance_id: param_string(params, &["instanceId", "instance_id"])?,
        name: param_optional_string(params, &["name"])?,
        working_dir: param_nullable_string_presence(params, &["workingDir", "working_dir"])?
            .flatten(),
        extra_args: param_optional_string(params, &["extraArgs", "extra_args"])?,
        bind_account_id: param_nullable_string_presence(
            params,
            &["bindAccountId", "bind_account_id"],
        )?,
    };
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        return update_default_instance_value(platform, params);
    }
    let updated = match platform {
        "antigravity" => cockpit_core::modules::instance::update_instance(payload),
        "github_copilot" => {
            cockpit_core::modules::github_copilot_instance::update_instance(payload)
        }
        "windsurf" => cockpit_core::modules::windsurf_instance::update_instance(payload),
        "kiro" => cockpit_core::modules::kiro_instance::update_instance(payload),
        "cursor" => cockpit_core::modules::cursor_instance::update_instance(payload),
        "gemini" => cockpit_core::modules::gemini_instance::update_instance(payload),
        "codebuddy" => cockpit_core::modules::codebuddy_instance::update_instance(payload),
        "codebuddy_cn" => cockpit_core::modules::codebuddy_cn_instance::update_instance(payload),
        "qoder" => cockpit_core::modules::qoder_instance::update_instance(payload),
        "trae" => cockpit_core::modules::trae_instance::update_instance(payload),
        "workbuddy" => cockpit_core::modules::workbuddy_instance::update_instance(payload),
        other => return Err(format!("unsupported instance platform: {other}")),
    }?;
    Ok(generic_instance_view_value(platform, updated))
}

fn delete_instance_for_platform(platform: &str, params: &Value) -> Result<Value, String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;
    if instance_id == DEFAULT_INSTANCE_ID {
        return Err("default instance cannot be deleted".to_string());
    }
    match platform {
        "antigravity" => cockpit_core::modules::instance::delete_instance(&instance_id),
        "codex" => cockpit_core::modules::codex_instance::delete_instance(&instance_id),
        "github_copilot" => {
            cockpit_core::modules::github_copilot_instance::delete_instance(&instance_id)
        }
        "windsurf" => cockpit_core::modules::windsurf_instance::delete_instance(&instance_id),
        "kiro" => cockpit_core::modules::kiro_instance::delete_instance(&instance_id),
        "cursor" => cockpit_core::modules::cursor_instance::delete_instance(&instance_id),
        "gemini" => cockpit_core::modules::gemini_instance::delete_instance(&instance_id),
        "codebuddy" => cockpit_core::modules::codebuddy_instance::delete_instance(&instance_id),
        "codebuddy_cn" => {
            cockpit_core::modules::codebuddy_cn_instance::delete_instance(&instance_id)
        }
        "qoder" => cockpit_core::modules::qoder_instance::delete_instance(&instance_id),
        "trae" => cockpit_core::modules::trae_instance::delete_instance(&instance_id),
        "workbuddy" => cockpit_core::modules::workbuddy_instance::delete_instance(&instance_id),
        other => return Err(format!("unsupported instance platform: {other}")),
    }?;
    Ok(Value::Null)
}

fn handle_instance_method(method: &str, params: &Value) -> Result<Option<Value>, String> {
    let Some((platform, action)) = method.split_once("/instance/") else {
        return Ok(None);
    };
    let platform = normalize_instance_platform(platform)?;
    let value = match action {
        "defaults/get" => instance_defaults_value(platform),
        "list" => list_instances_value(platform),
        "create" if platform == "codex" => create_codex_instance(params),
        "create" => create_generic_instance_value(platform, params),
        "update" if platform == "codex" => {
            let instance_id = param_string(params, &["instanceId", "instance_id"])?;
            if instance_id == DEFAULT_INSTANCE_ID {
                update_default_instance_value(platform, params)
            } else {
                update_codex_instance(params).map(|value| {
                    serde_json::from_value::<cockpit_core::models::InstanceProfile>(value.clone())
                        .map(|profile| codex_instance_view_value(profile, false, false))
                        .unwrap_or(value)
                })
            }
        }
        "update" => update_generic_instance_value(platform, params),
        "delete" => delete_instance_for_platform(platform, params),
        "stop" => stop_instance_for_platform(platform, params),
        "window/open" => open_instance_window_for_platform(platform, params),
        "close-all" => close_all_instances_for_platform(platform),
        "launch-command/get" => get_instance_launch_command_value(platform, params),
        "launch-command/execute" => execute_instance_launch_command_value(platform, params),
        "start" => start_instance_for_platform(platform, params),
        _ => return Ok(None),
    }?;
    Ok(Some(value))
}

fn get_provider_current_account_id(platform: &str) -> Result<Option<String>, String> {
    cockpit_core::modules::provider_current_state::resolve_provider_current_account_id(platform)
}

fn add_antigravity_account_from_refresh_token(params: &Value) -> Result<Value, String> {
    let refresh_token = param_string(params, &["refreshToken", "refresh_token"])?;
    let token_res = block_on(cockpit_core::modules::oauth::refresh_access_token(
        &refresh_token,
    ))?;
    let user_info = block_on(cockpit_core::modules::oauth::get_user_info(
        &token_res.access_token,
    ))?;
    let token = cockpit_core::models::TokenData::new(
        token_res.access_token,
        refresh_token,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None,
        None,
    )
    .with_oauth_metadata(token_res.oauth_client_key, token_res.id_token);
    let display_name = user_info.get_display_name();
    to_value_result(cockpit_core::modules::account::upsert_account(
        user_info.email,
        display_name,
        token,
    ))
}

fn antigravity_oauth_redirect_uri(service_addr: &str) -> String {
    let host = service_addr
        .rsplit_once('@')
        .map(|(_, value)| value)
        .unwrap_or(service_addr);
    format!("http://{host}/oauth-callback")
}

fn prepare_antigravity_oauth_url(service_addr: &str) -> Result<String, String> {
    if let Ok(state) = ANTIGRAVITY_OAUTH_STATE.lock() {
        if let Some(state) = state.as_ref() {
            publish_event("oauth-url-generated", Value::String(state.auth_url.clone()));
            return Ok(state.auth_url.clone());
        }
    }

    let redirect_uri = antigravity_oauth_redirect_uri(service_addr);
    let state_token = uuid::Uuid::new_v4().to_string();
    let auth_url = cockpit_core::modules::oauth::get_auth_url(&redirect_uri, Some(&state_token));
    let next = AntigravityOAuthState {
        auth_url: auth_url.clone(),
        redirect_uri,
        expected_state: state_token,
        code: None,
    };
    let mut state = ANTIGRAVITY_OAUTH_STATE
        .lock()
        .map_err(|_| "Antigravity OAuth state lock is poisoned".to_string())?;
    *state = Some(next);
    publish_event("oauth-url-generated", Value::String(auth_url.clone()));
    Ok(auth_url)
}

fn parse_antigravity_callback_url(
    raw_callback_url: &str,
    redirect_uri: &str,
) -> Result<url::Url, String> {
    let trimmed = raw_callback_url.trim();
    if trimmed.is_empty() {
        return Err("OAuth callback URL cannot be empty".to_string());
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return url::Url::parse(trimmed)
            .map_err(|err| format!("invalid OAuth callback URL: {err}"));
    }

    let redirect =
        url::Url::parse(redirect_uri).map_err(|err| format!("invalid redirect URI: {err}"))?;
    let host = redirect
        .host_str()
        .ok_or_else(|| "redirect URI is missing a host".to_string())?;
    let origin = match redirect.port() {
        Some(port) => format!("{}://{}:{}", redirect.scheme(), host, port),
        None => format!("{}://{}", redirect.scheme(), host),
    };

    if trimmed.starts_with('/') {
        return url::Url::parse(&format!("{origin}{trimmed}"))
            .map_err(|err| format!("invalid OAuth callback URL: {err}"));
    }

    url::Url::parse(&format!(
        "{origin}/oauth-callback?{}",
        trimmed.trim_start_matches('?')
    ))
    .map_err(|err| format!("invalid OAuth callback URL: {err}"))
}

fn extract_antigravity_oauth_code(
    callback_url: &url::Url,
    expected_state: &str,
) -> Result<String, String> {
    let mut code = None;
    let mut state = None;
    for (key, value) in callback_url.query_pairs() {
        match key.as_ref() {
            "code" if code.is_none() => code = Some(value.into_owned()),
            "state" if state.is_none() => state = Some(value.into_owned()),
            _ => {}
        }
    }

    let code = code
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "OAuth callback is missing authorization code".to_string())?;
    let state = state
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "OAuth callback is missing state".to_string())?;
    if state != expected_state {
        return Err("OAuth state validation failed".to_string());
    }
    Ok(code)
}

fn submit_antigravity_oauth_callback_url(params: &Value) -> Result<(), String> {
    let callback_url = param_string(params, &["callbackUrl", "callback_url"])?;
    submit_antigravity_oauth_callback_raw(&callback_url)
}

fn submit_antigravity_oauth_callback_raw(callback_url: &str) -> Result<(), String> {
    let (redirect_uri, expected_state) = {
        let state = ANTIGRAVITY_OAUTH_STATE
            .lock()
            .map_err(|_| "Antigravity OAuth state lock is poisoned".to_string())?;
        let state = state
            .as_ref()
            .ok_or_else(|| "OAuth state does not exist; start authorization first".to_string())?;
        (state.redirect_uri.clone(), state.expected_state.clone())
    };

    let parsed = parse_antigravity_callback_url(callback_url, &redirect_uri)?;
    if parsed.path() != "/oauth-callback" {
        return Err("OAuth callback path must be /oauth-callback".to_string());
    }
    let code = extract_antigravity_oauth_code(&parsed, &expected_state)?;

    let mut state = ANTIGRAVITY_OAUTH_STATE
        .lock()
        .map_err(|_| "Antigravity OAuth state lock is poisoned".to_string())?;
    let state = state
        .as_mut()
        .ok_or_else(|| "OAuth state does not exist; start authorization first".to_string())?;
    if state.expected_state != expected_state {
        return Err("OAuth state changed; restart authorization".to_string());
    }
    state.code = Some(code);
    publish_event("oauth-callback-received", Value::Null);
    Ok(())
}

fn antigravity_account_from_oauth_code(
    code: String,
    redirect_uri: String,
) -> Result<cockpit_core::models::Account, String> {
    let token_res = block_on(cockpit_core::modules::oauth::exchange_code(
        &code,
        &redirect_uri,
    ))?;
    let refresh_token = token_res
        .refresh_token
        .ok_or_else(|| "OAuth response did not include a refresh token".to_string())?;
    let user_info = block_on(cockpit_core::modules::oauth::get_user_info(
        &token_res.access_token,
    ))?;
    let token = cockpit_core::models::TokenData::new(
        token_res.access_token,
        refresh_token,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None,
        user_info.id.clone(),
    )
    .with_oauth_metadata(token_res.oauth_client_key, token_res.id_token);
    let display_name = user_info.get_display_name();
    let mut account =
        cockpit_core::modules::account::upsert_account(user_info.email, display_name, token)?;
    let account_id = account.id.clone();
    match block_on(async {
        cockpit_core::modules::account::fetch_quota_with_fresh_token(&mut account, true)
            .await
            .map_err(|err| err.to_string())
    }) {
        Ok(quota) => {
            if cockpit_core::modules::account::update_account_quota(&account_id, quota).is_ok() {
                if let Ok(updated) = cockpit_core::modules::account::load_account(&account_id) {
                    account = updated;
                }
            }
        }
        Err(err) => {
            cockpit_core::modules::logger::log_warn(&format!(
                "Antigravity OAuth quota refresh failed after login: account_id={account_id}, error={err}"
            ));
        }
    }
    Ok(account)
}

fn complete_antigravity_oauth_login() -> Result<Value, String> {
    let (code, redirect_uri) = {
        let state = ANTIGRAVITY_OAUTH_STATE
            .lock()
            .map_err(|_| "Antigravity OAuth state lock is poisoned".to_string())?;
        let state = state
            .as_ref()
            .ok_or_else(|| "OAuth state does not exist; start authorization first".to_string())?;
        let code = state
            .code
            .clone()
            .ok_or_else(|| "OAuth authorization has not completed yet".to_string())?;
        (code, state.redirect_uri.clone())
    };
    let account = antigravity_account_from_oauth_code(code, redirect_uri)?;
    let mut state = ANTIGRAVITY_OAUTH_STATE
        .lock()
        .map_err(|_| "Antigravity OAuth state lock is poisoned".to_string())?;
    *state = None;
    to_value_result(Ok(account))
}

fn start_antigravity_oauth_login(service_addr: &str) -> Result<Value, String> {
    let _ = prepare_antigravity_oauth_url(service_addr)?;
    complete_antigravity_oauth_login()
}

fn cancel_antigravity_oauth_login() -> Result<(), String> {
    let mut state = ANTIGRAVITY_OAUTH_STATE
        .lock()
        .map_err(|_| "Antigravity OAuth state lock is poisoned".to_string())?;
    *state = None;
    Ok(())
}

fn fetch_antigravity_account_quota(params: &Value) -> Result<Value, String> {
    let account_id = param_account_id(params)?;
    let mut account = cockpit_core::modules::account::load_account(&account_id)?;
    let quota = block_on(async {
        cockpit_core::modules::account::fetch_quota_with_fresh_token(&mut account, true)
            .await
            .map_err(|err| err.to_string())
    })?;
    cockpit_core::modules::account::update_account_quota(&account_id, quota)?;
    to_value_result(cockpit_core::modules::account::load_account(&account_id))
}

fn refresh_current_antigravity_quota() -> Result<Value, String> {
    let Some(mut account) = cockpit_core::modules::account::get_current_account()? else {
        return Err("current Antigravity account not found".to_string());
    };
    let account_id = account.id.clone();
    let quota = block_on(async {
        cockpit_core::modules::account::fetch_quota_with_fresh_token(&mut account, true)
            .await
            .map_err(|err| err.to_string())
    })?;
    cockpit_core::modules::account::update_account_quota(&account_id, quota)?;
    to_value_result(Ok::<_, String>(()))
}

fn refresh_current_codex_quota() -> Result<Value, String> {
    let Some(account) = cockpit_core::modules::codex_account::get_current_account() else {
        return Err("current Codex account not found".to_string());
    };
    if account.is_api_key_auth() {
        return to_value_result(Ok::<_, String>(()));
    }
    block_on(cockpit_core::modules::codex_quota::refresh_account_quota(
        &account.id,
    ))?;
    to_value_result(Ok::<_, String>(()))
}

fn add_codex_account_with_token(params: &Value) -> Result<Value, String> {
    let id_token = param_string(params, &["idToken", "id_token"])?;
    let access_token = param_access_token(params)?;
    let refresh_token = param_optional_string(params, &["refreshToken", "refresh_token"])?;
    let account = cockpit_core::modules::codex_account::upsert_account(
        cockpit_core::models::codex::CodexTokens {
            id_token,
            access_token,
            refresh_token,
        },
    )?;
    let _ = block_on(cockpit_core::modules::codex_quota::refresh_account_quota(
        &account.id,
    ));
    to_value_result(
        cockpit_core::modules::codex_account::load_account(&account.id)
            .ok_or_else(|| "Codex account was saved but could not be loaded".to_string()),
    )
}

fn start_codex_oauth_callback_listener(login_id: &str) -> Result<(), String> {
    {
        let active = CODEX_OAUTH_LISTENER_LOGIN_ID
            .lock()
            .map_err(|_| "Codex OAuth listener lock is poisoned".to_string())?;
        if active.as_deref() == Some(login_id) {
            return Ok(());
        }
    }

    let port = cockpit_core::modules::codex_oauth::get_callback_port();
    let server = Server::http(format!("127.0.0.1:{port}"))
        .map_err(|err| format!("start Codex OAuth callback listener failed: {err}"))?;
    {
        let mut active = CODEX_OAUTH_LISTENER_LOGIN_ID
            .lock()
            .map_err(|_| "Codex OAuth listener lock is poisoned".to_string())?;
        *active = Some(login_id.to_string());
    }

    let login_id_for_thread = login_id.to_string();
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let url = request.url().to_string();
            if url.starts_with("/auth/callback") {
                let result = cockpit_core::modules::codex_oauth::submit_callback_url(
                    &login_id_for_thread,
                    &url,
                );
                let response = match result {
                    Ok(()) => {
                        let payload = json!({ "loginId": login_id_for_thread.clone() });
                        publish_event("codex-oauth-login-completed", payload.clone());
                        publish_event("ghcp-oauth-login-completed", payload);
                        Response::from_string(
                            "<!doctype html><html><body><h1>Authorization complete</h1><p>You can close this page and return to Cockpit Tools.</p></body></html>",
                        )
                        .with_status_code(StatusCode(200))
                    }
                    Err(err) => Response::from_string(format!(
                        "<!doctype html><html><body><h1>Authorization failed</h1><p>{err}</p></body></html>"
                    ))
                    .with_status_code(StatusCode(400)),
                };
                let _ = request.respond(response);
                break;
            }
            if url.starts_with("/cancel") {
                let _ = request.respond(Response::from_string("Login cancelled"));
                break;
            }
            let _ = request.respond(Response::from_string("Not Found").with_status_code(404));
        }

        if let Ok(mut active) = CODEX_OAUTH_LISTENER_LOGIN_ID.lock() {
            if active.as_deref() == Some(&login_id_for_thread) {
                *active = None;
            }
        }
    });
    Ok(())
}

fn codex_oauth_login_start() -> Result<Value, String> {
    let response = cockpit_core::modules::codex_oauth::start_oauth_login_headless()?;
    if let Err(err) = start_codex_oauth_callback_listener(&response.login_id) {
        let _ = cockpit_core::modules::codex_oauth::cancel_oauth_flow_for(Some(
            response.login_id.as_str(),
        ));
        return Err(err);
    }
    to_value_result(Ok(response))
}

fn codex_oauth_login_completed(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let tokens = block_on(cockpit_core::modules::codex_oauth::complete_oauth_login(
        &login_id,
    ))?;
    let account = cockpit_core::modules::codex_account::upsert_account(tokens)?;
    if let Err(err) = block_on(cockpit_core::modules::codex_quota::refresh_account_quota(
        &account.id,
    )) {
        cockpit_core::modules::logger::log_warn(&format!(
            "Codex OAuth quota refresh failed after login: account_id={}, error={err}",
            account.id
        ));
    }
    let loaded = cockpit_core::modules::codex_account::load_account(&account.id)
        .ok_or_else(|| "Codex account could not be loaded after OAuth save".to_string())?;
    to_value_result(Ok(loaded))
}

fn codex_oauth_login_cancel(params: &Value) -> Result<(), String> {
    let login_id = param_optional_login_id(params)?;
    cockpit_core::modules::codex_oauth::cancel_oauth_flow_for(login_id.as_deref())
}

fn codex_oauth_submit_callback_url(params: &Value) -> Result<(), String> {
    let login_id = param_login_id(params)?;
    let callback_url = param_callback_url(params)?;
    cockpit_core::modules::codex_oauth::submit_callback_url(&login_id, &callback_url)?;
    let payload = json!({ "loginId": login_id });
    publish_event("codex-oauth-login-completed", payload.clone());
    publish_event("ghcp-oauth-login-completed", payload);
    Ok(())
}

fn is_codex_oauth_port_in_use() -> Result<bool, String> {
    cockpit_core::modules::process::is_port_in_use(
        cockpit_core::modules::codex_oauth::get_callback_port(),
    )
}

fn close_codex_oauth_port() -> Result<u32, String> {
    cockpit_core::modules::process::kill_port_processes(
        cockpit_core::modules::codex_oauth::get_callback_port(),
    )
    .map(|count| count as u32)
}

fn add_codex_account_with_api_key(params: &Value) -> Result<Value, String> {
    let api_key = param_string(params, &["apiKey", "api_key"])?;
    let api_base_url = param_optional_string(params, &["apiBaseUrl", "api_base_url"])?;
    let account = cockpit_core::modules::codex_account::upsert_api_key_account(
        api_key,
        api_base_url,
        param_optional_codex_api_provider_mode(params)?,
        param_optional_string(params, &["apiProviderId", "api_provider_id"])?,
        param_optional_string(params, &["apiProviderName", "api_provider_name"])?,
        param_optional_string_vec(params, &["apiModelCatalog", "api_model_catalog"])?
            .unwrap_or_default(),
        param_optional_string(params, &["apiWireApi", "api_wire_api"])?,
        param_optional_bool(params, &["apiSupportsVision", "api_supports_vision"])?
            .unwrap_or(false),
        param_optional_bool_map(
            params,
            &["apiModelVisionSupport", "api_model_vision_support"],
        )?,
        param_optional_string(
            params,
            &["apiVisionRoutingModel", "api_vision_routing_model"],
        )?,
        param_optional_string(params, &["accountName", "account_name"])?,
    )?;
    to_value_result(
        cockpit_core::modules::codex_account::load_account(&account.id)
            .ok_or_else(|| "Codex API key account was saved but could not be loaded".to_string()),
    )
}

fn update_codex_api_key_credentials(params: &Value) -> Result<Value, String> {
    let account_id = param_account_id(params)?;
    let api_key = param_string(params, &["apiKey", "api_key"])?;
    let account = cockpit_core::modules::codex_account::update_api_key_credentials(
        &account_id,
        api_key,
        param_optional_string(params, &["apiBaseUrl", "api_base_url"])?,
        param_optional_codex_api_provider_mode(params)?,
        param_optional_string(params, &["apiProviderId", "api_provider_id"])?,
        param_optional_string(params, &["apiProviderName", "api_provider_name"])?,
        param_optional_string_vec(params, &["apiModelCatalog", "api_model_catalog"])?
            .unwrap_or_default(),
        param_optional_string(params, &["apiWireApi", "api_wire_api"])?,
        param_optional_bool(params, &["apiSupportsVision", "api_supports_vision"])?
            .unwrap_or(false),
        param_optional_bool_map(
            params,
            &["apiModelVisionSupport", "api_model_vision_support"],
        )?,
        param_optional_string(
            params,
            &["apiVisionRoutingModel", "api_vision_routing_model"],
        )?,
    )?;
    to_value_result(Ok::<_, String>(account))
}

fn update_codex_api_key_bound_oauth_account(params: &Value) -> Result<Value, String> {
    let account_id = param_account_id(params)?;
    let bound_oauth_account_id =
        param_optional_string(params, &["boundOauthAccountId", "bound_oauth_account_id"])?;
    to_value_result(block_on(
        cockpit_core::modules::codex_account::update_api_key_bound_oauth_account(
            &account_id,
            bound_oauth_account_id,
        ),
    ))
}

fn update_codex_account_app_speed(params: &Value) -> Result<Value, String> {
    let account_id = param_account_id(params)?;
    let speed = param_codex_app_speed(params)?;
    to_value_result(
        cockpit_core::modules::codex_account::update_account_app_speed(&account_id, speed),
    )
}

fn add_cursor_account_with_token(params: &Value) -> Result<Value, String> {
    let payload = cockpit_core::models::cursor::CursorImportPayload {
        email: "unknown".to_string(),
        auth_id: None,
        name: None,
        access_token: param_access_token(params)?,
        refresh_token: None,
        membership_type: None,
        subscription_status: None,
        sign_up_type: None,
        cursor_auth_raw: None,
        cursor_usage_raw: None,
        status: None,
        status_reason: None,
    };
    to_value_result(cockpit_core::modules::cursor_account::upsert_account(
        payload,
    ))
}

fn add_gemini_account_with_token(params: &Value) -> Result<Value, String> {
    let payload = cockpit_core::models::gemini::GeminiOAuthCompletePayload {
        email: "unknown@gmail.com".to_string(),
        auth_id: None,
        name: None,
        access_token: param_access_token(params)?,
        refresh_token: None,
        id_token: None,
        token_type: None,
        scope: None,
        expiry_date: None,
        selected_auth_type: Some("oauth-personal".to_string()),
        project_id: None,
        tier_id: None,
        plan_name: None,
        gemini_auth_raw: None,
        gemini_usage_raw: None,
        status: None,
        status_reason: None,
    };
    let mut account = cockpit_core::modules::gemini_account::upsert_account(payload)?;
    match block_on(cockpit_core::modules::gemini_account::refresh_account_token(&account.id)) {
        Ok(refreshed) => account = refreshed,
        Err(error) => {
            let _ = cockpit_core::modules::gemini_account::set_account_status(
                &account.id,
                Some("error"),
                Some(&error),
            );
            account.status = Some("error".to_string());
            account.status_reason = Some(error);
        }
    }
    to_value_result(Ok::<_, String>(account))
}

fn add_trae_account_with_token(params: &Value) -> Result<Value, String> {
    let payload = cockpit_core::models::trae::TraeImportPayload {
        email: "unknown".to_string(),
        user_id: None,
        nickname: None,
        access_token: param_access_token(params)?,
        refresh_token: None,
        token_type: None,
        expires_at: None,
        plan_type: None,
        plan_reset_at: None,
        trae_auth_raw: None,
        trae_profile_raw: None,
        trae_entitlement_raw: None,
        trae_usage_raw: None,
        trae_server_raw: None,
        trae_usertag_raw: None,
        status: None,
        status_reason: None,
    };
    to_value_result(cockpit_core::modules::trae_account::upsert_account(payload))
}

fn add_github_copilot_account_with_token(params: &Value) -> Result<Value, String> {
    let token = param_access_token(params)?;
    let payload = block_on(
        cockpit_core::modules::github_copilot_oauth::build_payload_from_github_access_token(&token),
    )?;
    to_value_result(cockpit_core::modules::github_copilot_account::upsert_account(payload))
}

fn add_windsurf_account_with_token(params: &Value) -> Result<Value, String> {
    let token = param_access_token(params)?;
    let payload =
        block_on(cockpit_core::modules::windsurf_oauth::build_payload_from_token(&token))?;
    to_value_result(cockpit_core::modules::windsurf_account::upsert_account(
        payload,
    ))
}

fn add_windsurf_account_with_password(params: &Value) -> Result<Value, String> {
    let email = param_string(params, &["email"])?;
    let password = param_string(params, &["password"])?;
    let payload = block_on(
        cockpit_core::modules::windsurf_oauth::build_payload_from_password(&email, &password),
    )?;
    to_value_result(cockpit_core::modules::windsurf_account::upsert_account(
        payload,
    ))
}

#[derive(Debug, Deserialize)]
struct WindsurfPasswordCredentialInput {
    email: String,
    password: String,
    #[serde(default, alias = "sourceLine")]
    source_line: Option<usize>,
}

#[derive(Debug, Serialize)]
struct WindsurfPasswordCredentialFailure {
    email: String,
    error: String,
    source_line: Option<usize>,
}

fn add_windsurf_accounts_with_password(params: &Value) -> Result<Value, String> {
    let credentials_value = params
        .as_object()
        .and_then(|obj| obj.get("credentials"))
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    let credentials: Vec<WindsurfPasswordCredentialInput> =
        serde_json::from_value(credentials_value)
            .map_err(|err| format!("credentials must be a password credential array: {err}"))?;
    if credentials.is_empty() {
        return Err("please provide at least one email/password credential".to_string());
    }

    let mut accounts = Vec::new();
    let mut failures = Vec::new();
    for item in credentials {
        let email = item.email.trim().to_string();
        if email.is_empty() || item.password.is_empty() {
            failures.push(WindsurfPasswordCredentialFailure {
                email,
                error: "email and password cannot be empty".to_string(),
                source_line: item.source_line,
            });
            continue;
        }

        let result = block_on(
            cockpit_core::modules::windsurf_oauth::build_payload_from_password(
                &email,
                &item.password,
            ),
        )
        .and_then(cockpit_core::modules::windsurf_account::upsert_account);

        match result {
            Ok(account) => accounts.push(account),
            Err(error) => failures.push(WindsurfPasswordCredentialFailure {
                email,
                error,
                source_line: item.source_line,
            }),
        }
    }

    to_value_result(Ok::<_, String>(json!({
        "success_count": accounts.len(),
        "failed_count": failures.len(),
        "accounts": accounts,
        "failures": failures,
    })))
}

fn add_kiro_account_with_token(params: &Value) -> Result<Value, String> {
    let token = param_access_token(params)?;
    let payload = block_on(cockpit_core::modules::kiro_oauth::build_payload_from_token(
        &token,
    ))?;
    to_value_result(cockpit_core::modules::kiro_account::upsert_account(payload))
}

fn add_codebuddy_account_with_token(params: &Value) -> Result<Value, String> {
    let token = param_access_token(params)?;
    let payload =
        block_on(cockpit_core::modules::codebuddy_oauth::build_payload_from_token(&token))?;
    to_value_result(cockpit_core::modules::codebuddy_account::upsert_account(
        payload,
    ))
}

fn add_codebuddy_cn_account_with_token(params: &Value) -> Result<Value, String> {
    let token = param_access_token(params)?;
    let payload =
        block_on(cockpit_core::modules::codebuddy_cn_oauth::build_payload_from_token(&token))?;
    to_value_result(cockpit_core::modules::codebuddy_cn_account::upsert_account(
        payload,
    ))
}

fn add_workbuddy_account_with_token(params: &Value) -> Result<Value, String> {
    let token = param_access_token(params)?;
    let payload =
        block_on(cockpit_core::modules::workbuddy_oauth::build_payload_from_token(&token))?;
    to_value_result(cockpit_core::modules::workbuddy_account::upsert_account(
        payload,
    ))
}

fn import_github_copilot_from_local() -> Result<Value, String> {
    let account = block_on(cockpit_core::modules::github_copilot_account::import_from_local())?
        .ok_or_else(|| {
            "local VS Code GitHub Copilot account not found on service host".to_string()
        })?;
    to_value_result(Ok::<_, String>(vec![account]))
}

fn import_windsurf_from_local() -> Result<Value, String> {
    let auth_status = cockpit_core::modules::windsurf_account::read_local_auth_status()?
        .ok_or_else(|| "local Windsurf login not found on service host".to_string())?;
    let mut payload = block_on(
        cockpit_core::modules::windsurf_oauth::build_payload_from_local_auth_status(auth_status),
    )?;
    if payload.github_login.trim().is_empty() {
        if let Some(hint) = cockpit_core::modules::windsurf_account::read_local_login_hint() {
            payload.github_login = hint;
        }
    }
    to_value_result(
        cockpit_core::modules::windsurf_account::upsert_account(payload)
            .map(|account| vec![account]),
    )
}

fn import_kiro_from_local() -> Result<Value, String> {
    let payload = cockpit_core::modules::kiro_oauth::build_payload_from_local_files()?;
    let payload = block_on_value(
        cockpit_core::modules::kiro_oauth::enrich_payload_with_runtime_usage(payload),
    )?;
    to_value_result(
        cockpit_core::modules::kiro_account::upsert_account(payload).map(|account| vec![account]),
    )
}

fn merge_codebuddy_local_payload(
    mut local_payload: cockpit_core::models::codebuddy::CodebuddyOAuthCompletePayload,
    fetched_payload: cockpit_core::models::codebuddy::CodebuddyOAuthCompletePayload,
) -> cockpit_core::models::codebuddy::CodebuddyOAuthCompletePayload {
    let mut payload = fetched_payload;
    if payload.uid.is_none() {
        payload.uid = local_payload.uid.clone();
    }
    if payload.nickname.is_none() {
        payload.nickname = local_payload.nickname.clone();
    }
    if payload.refresh_token.is_none() {
        payload.refresh_token = local_payload.refresh_token.clone();
    }
    if payload.domain.is_none() {
        payload.domain = local_payload.domain.clone();
    }
    if payload.token_type.is_none() {
        payload.token_type = local_payload.token_type.clone();
    }
    if payload.expires_at.is_none() {
        payload.expires_at = local_payload.expires_at;
    }
    if payload.auth_raw.is_none() {
        payload.auth_raw = local_payload.auth_raw.take();
    }
    if payload.profile_raw.is_none() {
        payload.profile_raw = local_payload.profile_raw.take();
    }
    if payload.email.trim().is_empty() || payload.email == "unknown" {
        payload.email = local_payload.email;
    }
    payload
}

fn is_codebuddy_placeholder(account: &cockpit_core::models::codebuddy::CodebuddyAccount) -> bool {
    account.email.trim().eq_ignore_ascii_case("unknown")
        || account.email.trim().is_empty()
        || account
            .uid
            .as_deref()
            .map(str::trim)
            .map(str::is_empty)
            .unwrap_or(true)
}

fn import_codebuddy_from_local() -> Result<Value, String> {
    let mut local_payload = cockpit_core::modules::codebuddy_account::import_payload_from_local()?
        .ok_or_else(|| "local CodeBuddy login not found on service host".to_string())?;

    match block_on(
        cockpit_core::modules::codebuddy_oauth::build_payload_from_token(
            &local_payload.access_token,
        ),
    ) {
        Ok(payload) => local_payload = merge_codebuddy_local_payload(local_payload, payload),
        Err(err) => cockpit_core::modules::logger::log_warn(&format!(
            "[CodeBuddy Import Local] fetch profile failed, using local payload: {err}"
        )),
    }

    let mut account = cockpit_core::modules::codebuddy_account::upsert_account(local_payload)?;
    for existing in cockpit_core::modules::codebuddy_account::list_accounts() {
        if existing.id == account.id || existing.access_token != account.access_token {
            continue;
        }
        if is_codebuddy_placeholder(&existing) {
            let _ = cockpit_core::modules::codebuddy_account::remove_account(&existing.id);
        }
    }
    if let Ok(refreshed) =
        block_on(cockpit_core::modules::codebuddy_account::refresh_account_token(&account.id))
    {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(vec![account]))
}

fn import_codebuddy_cn_from_local() -> Result<Value, String> {
    let mut local_payload =
        cockpit_core::modules::codebuddy_cn_account::import_payload_from_local()?
            .ok_or_else(|| "local CodeBuddy CN login not found on service host".to_string())?;

    match block_on(
        cockpit_core::modules::codebuddy_cn_oauth::build_payload_from_token(
            &local_payload.access_token,
        ),
    ) {
        Ok(payload) => local_payload = merge_codebuddy_local_payload(local_payload, payload),
        Err(err) => cockpit_core::modules::logger::log_warn(&format!(
            "[CodeBuddy CN Import Local] fetch profile failed, using local payload: {err}"
        )),
    }

    let mut account = cockpit_core::modules::codebuddy_cn_account::upsert_account(local_payload)?;
    for existing in cockpit_core::modules::codebuddy_cn_account::list_accounts() {
        if existing.id == account.id || existing.access_token != account.access_token {
            continue;
        }
        if is_codebuddy_placeholder(&existing) {
            let _ = cockpit_core::modules::codebuddy_cn_account::remove_account(&existing.id);
        }
    }
    if let Ok(refreshed) =
        block_on(cockpit_core::modules::codebuddy_cn_account::refresh_account_token(&account.id))
    {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(vec![account]))
}

fn merge_workbuddy_local_payload(
    mut local_payload: cockpit_core::models::workbuddy::WorkbuddyOAuthCompletePayload,
    fetched_payload: cockpit_core::models::workbuddy::WorkbuddyOAuthCompletePayload,
) -> cockpit_core::models::workbuddy::WorkbuddyOAuthCompletePayload {
    let mut payload = fetched_payload;
    if payload.uid.is_none() {
        payload.uid = local_payload.uid.clone();
    }
    if payload.nickname.is_none() {
        payload.nickname = local_payload.nickname.clone();
    }
    if payload.refresh_token.is_none() {
        payload.refresh_token = local_payload.refresh_token.clone();
    }
    if payload.domain.is_none() {
        payload.domain = local_payload.domain.clone();
    }
    if payload.token_type.is_none() {
        payload.token_type = local_payload.token_type.clone();
    }
    if payload.expires_at.is_none() {
        payload.expires_at = local_payload.expires_at;
    }
    if payload.auth_raw.is_none() {
        payload.auth_raw = local_payload.auth_raw.take();
    }
    if payload.profile_raw.is_none() {
        payload.profile_raw = local_payload.profile_raw.take();
    }
    if payload.email.trim().is_empty() || payload.email == "unknown" {
        payload.email = local_payload.email;
    }
    payload
}

fn is_workbuddy_placeholder(account: &cockpit_core::models::workbuddy::WorkbuddyAccount) -> bool {
    account.email.trim().eq_ignore_ascii_case("unknown")
        || account.email.trim().is_empty()
        || account
            .uid
            .as_deref()
            .map(str::trim)
            .map(str::is_empty)
            .unwrap_or(true)
}

fn import_workbuddy_from_local() -> Result<Value, String> {
    let mut local_payload = cockpit_core::modules::workbuddy_account::import_payload_from_local()?
        .ok_or_else(|| "local WorkBuddy login not found on service host".to_string())?;

    match block_on(
        cockpit_core::modules::workbuddy_oauth::build_payload_from_token(
            &local_payload.access_token,
        ),
    ) {
        Ok(payload) => local_payload = merge_workbuddy_local_payload(local_payload, payload),
        Err(err) => cockpit_core::modules::logger::log_warn(&format!(
            "[WorkBuddy Import Local] fetch profile failed, using local payload: {err}"
        )),
    }

    let mut account = cockpit_core::modules::workbuddy_account::upsert_account(local_payload)?;
    for existing in cockpit_core::modules::workbuddy_account::list_accounts() {
        if existing.id == account.id || existing.access_token != account.access_token {
            continue;
        }
        if is_workbuddy_placeholder(&existing) {
            let _ = cockpit_core::modules::workbuddy_account::remove_account(&existing.id);
        }
    }
    if let Ok(refreshed) =
        block_on(cockpit_core::modules::workbuddy_account::refresh_account_token(&account.id))
    {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(vec![account]))
}

fn complete_cursor_oauth_login(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let payload = block_on(cockpit_core::modules::cursor_oauth::complete_login(
        &login_id,
    ))?;
    let mut account = cockpit_core::modules::cursor_account::upsert_account(payload)?;
    if let Ok(refreshed) =
        block_on(cockpit_core::modules::cursor_account::refresh_account_async(&account.id))
    {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(account))
}

fn complete_gemini_oauth_login(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let payload = block_on(cockpit_core::modules::gemini_oauth::complete_login(
        &login_id,
    ))?;
    let mut account = cockpit_core::modules::gemini_account::upsert_account(payload)?;
    match block_on(cockpit_core::modules::gemini_account::refresh_account_token(&account.id)) {
        Ok(refreshed) => account = refreshed,
        Err(error) => {
            let _ = cockpit_core::modules::gemini_account::set_account_status(
                &account.id,
                Some("error"),
                Some(&error),
            );
            account.status = Some("error".to_string());
            account.status_reason = Some(error);
        }
    }
    to_value_result(Ok::<_, String>(account))
}

fn complete_github_copilot_oauth_login(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let payload = block_on(cockpit_core::modules::github_copilot_oauth::complete_login(
        &login_id,
    ))?;
    let mut account = cockpit_core::modules::github_copilot_account::upsert_account(payload)?;
    if let Ok(refreshed) =
        block_on(cockpit_core::modules::github_copilot_account::refresh_account_token(&account.id))
    {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(account))
}

fn complete_windsurf_oauth_login(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let payload = block_on(cockpit_core::modules::windsurf_oauth::complete_login(
        &login_id,
    ))?;
    let mut account = cockpit_core::modules::windsurf_account::upsert_account(payload)?;
    if let Ok(refreshed) =
        block_on(cockpit_core::modules::windsurf_account::refresh_account_token(&account.id))
    {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(account))
}

fn complete_kiro_oauth_login(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let payload = block_on(cockpit_core::modules::kiro_oauth::complete_login(&login_id))?;
    let mut account = cockpit_core::modules::kiro_account::upsert_account(payload)?;
    if let Ok(refreshed) = block_on(cockpit_core::modules::kiro_account::refresh_account_token(
        &account.id,
    )) {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(account))
}

fn complete_codebuddy_oauth_login(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let payload = block_on(cockpit_core::modules::codebuddy_oauth::complete_login(
        &login_id,
    ))?;
    let mut account = cockpit_core::modules::codebuddy_account::upsert_account(payload)?;
    if let Ok(refreshed) =
        block_on(cockpit_core::modules::codebuddy_account::refresh_account_token(&account.id))
    {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(account))
}

fn complete_codebuddy_cn_oauth_login(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let payload = block_on(cockpit_core::modules::codebuddy_cn_oauth::complete_login(
        &login_id,
    ))?;
    let mut account = cockpit_core::modules::codebuddy_cn_account::upsert_account(payload)?;
    if let Ok(refreshed) =
        block_on(cockpit_core::modules::codebuddy_cn_account::refresh_account_token(&account.id))
    {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(account))
}

fn complete_workbuddy_oauth_login(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let payload = block_on(cockpit_core::modules::workbuddy_oauth::complete_login(
        &login_id,
    ))?;
    let mut account = cockpit_core::modules::workbuddy_account::upsert_account(payload)?;
    if let Ok(refreshed) =
        block_on(cockpit_core::modules::workbuddy_account::refresh_account_token(&account.id))
    {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(account))
}

fn complete_trae_oauth_login(params: &Value) -> Result<Value, String> {
    let login_id = param_login_id(params)?;
    let payload = block_on(cockpit_core::modules::trae_oauth::complete_login(&login_id))?;
    let mut account = cockpit_core::modules::trae_account::upsert_account(payload)?;
    if let Ok(refreshed) = block_on(cockpit_core::modules::trae_account::refresh_account_async(
        &account.id,
    )) {
        account = refreshed;
    }
    to_value_result(Ok::<_, String>(account))
}

fn inject_provider_account(platform: &str, account_id: &str) -> Result<Value, String> {
    let message = match platform {
        "cursor" => {
            let account = cockpit_core::modules::cursor_account::load_account(account_id)
                .ok_or_else(|| format!("Cursor account not found: {account_id}"))?;
            cockpit_core::modules::cursor_account::inject_to_cursor(account_id)?;
            cockpit_core::modules::provider_current_state::set_current_account_id(
                "cursor",
                Some(account_id),
            )?;
            let _ = cockpit_core::modules::cursor_instance::update_default_settings(
                Some(Some(account_id.to_string())),
                None,
                Some(false),
            );
            format!("Switched on service host: {}", account.email)
        }
        "gemini" => {
            let account = cockpit_core::modules::gemini_account::load_account(account_id)
                .ok_or_else(|| format!("Gemini account not found: {account_id}"))?;
            cockpit_core::modules::gemini_account::inject_to_gemini(account_id)?;
            cockpit_core::modules::provider_current_state::set_current_account_id(
                "gemini",
                Some(account_id),
            )?;
            format!("Switched on service host: {}", account.email)
        }
        "github_copilot" => {
            let account =
                cockpit_core::modules::github_copilot_account::load_account(account_id)
                    .ok_or_else(|| format!("GitHub Copilot account not found: {account_id}"))?;
            let _ = cockpit_core::modules::github_copilot_instance::update_default_settings(
                Some(Some(account_id.to_string())),
                None,
                Some(false),
            );
            cockpit_core::modules::provider_current_state::set_current_account_id(
                "github_copilot",
                Some(account_id),
            )?;
            format!(
                "Bound default service-host VS Code profile: {}",
                account.github_login
            )
        }
        "windsurf" => {
            let account = cockpit_core::modules::windsurf_account::load_account(account_id)
                .ok_or_else(|| format!("Windsurf account not found: {account_id}"))?;
            let _ = cockpit_core::modules::windsurf_instance::update_default_settings(
                Some(Some(account_id.to_string())),
                None,
                Some(false),
            );
            cockpit_core::modules::provider_current_state::set_current_account_id(
                "windsurf",
                Some(account_id),
            )?;
            format!(
                "Bound default service-host Windsurf profile: {}",
                account.github_login
            )
        }
        "kiro" => {
            let account = cockpit_core::modules::kiro_account::load_account(account_id)
                .ok_or_else(|| format!("Kiro account not found: {account_id}"))?;
            let _ = cockpit_core::modules::kiro_instance::update_default_settings(
                Some(Some(account_id.to_string())),
                None,
                Some(false),
            );
            cockpit_core::modules::provider_current_state::set_current_account_id(
                "kiro",
                Some(account_id),
            )?;
            format!("Bound default service-host Kiro profile: {}", account.email)
        }
        "qoder" => {
            let account = cockpit_core::modules::qoder_account::load_account(account_id)
                .ok_or_else(|| format!("Qoder account not found: {account_id}"))?;
            cockpit_core::modules::qoder_account::inject_to_qoder(account_id)?;
            cockpit_core::modules::provider_current_state::set_current_account_id(
                "qoder",
                Some(account_id),
            )?;
            let _ = cockpit_core::modules::qoder_instance::update_default_settings(
                Some(Some(account_id.to_string())),
                None,
                Some(false),
            );
            format!("Switched on service host: {}", account.email)
        }
        "trae" => {
            let account = block_on(cockpit_core::modules::trae_account::refresh_account_async(
                account_id,
            ))?;
            cockpit_core::modules::trae_account::inject_to_trae(account_id)?;
            cockpit_core::modules::provider_current_state::set_current_account_id(
                "trae",
                Some(account_id),
            )?;
            let _ = cockpit_core::modules::trae_instance::update_default_settings(
                Some(Some(account_id.to_string())),
                None,
                Some(false),
            );
            format!("Switched on service host: {}", account.email)
        }
        "zed" => {
            let account = cockpit_core::modules::zed_account::inject_account(account_id)?;
            match cockpit_core::modules::zed_instance::restart_default_session() {
                Ok(_) => format!("Switched on service host: {}", account.github_login),
                Err(err)
                    if err.starts_with("APP_PATH_NOT_FOUND:")
                        || err.contains("鍚姩 Zed 澶辫触") =>
                {
                    publish_event(
                        "app:path_missing",
                        json!({
                            "app": "zed",
                            "retry": { "kind": "switchAccount", "accountId": account_id }
                        }),
                    );
                    format!("Switched on service host, but Zed restart failed: {}", err)
                }
                Err(err) => return Err(err),
            }
        }
        other => return Err(format!("unsupported inject platform: {other}")),
    };
    to_value_result(Ok::<_, String>(message))
}

fn save_group_settings_from_params(params: &Value) -> Result<Value, String> {
    to_value_result(
        cockpit_core::modules::group_settings::replace_group_settings(
            param_string_map(params, "groupMappings")?,
            param_string_map(params, "groupNames")?,
            param_string_vec(params, &["groupOrder", "group_order"])?,
        ),
    )
}

fn mutate_group_settings<F>(mutator: F) -> Result<Value, String>
where
    F: FnOnce(&mut cockpit_core::modules::group_settings::GroupSettings),
{
    to_value_result(cockpit_core::modules::group_settings::mutate_group_settings(mutator))
}

fn get_display_groups() -> Value {
    json!(cockpit_core::modules::group_settings::get_display_groups())
}

fn delete_corrupted_file(params: &Value) -> Result<(), String> {
    let path = param_string(params, &["path"])?;
    cockpit_core::modules::corrupted_file::backup_corrupted_file(path).map(|_| ())
}

fn get_workbuddy_checkin_status(params: &Value) -> Result<Value, String> {
    let account_id = param_account_id(params)?;
    let account = cockpit_core::modules::workbuddy_account::load_account(&account_id)
        .ok_or_else(|| format!("account not found: {account_id}"))?;
    to_value_result(block_on(
        cockpit_core::modules::codebuddy_cn_oauth::get_checkin_status(
            &account.access_token,
            account.uid.as_deref(),
            account.enterprise_id.as_deref(),
            account.domain.as_deref(),
        ),
    ))
}

fn run_workbuddy_checkin(params: &Value) -> Result<Value, String> {
    let account_id = param_account_id(params)?;
    let account = cockpit_core::modules::workbuddy_account::load_account(&account_id)
        .ok_or_else(|| format!("account not found: {account_id}"))?;
    let response = block_on(cockpit_core::modules::codebuddy_cn_oauth::perform_checkin(
        &account.access_token,
        account.uid.as_deref(),
        account.enterprise_id.as_deref(),
        account.domain.as_deref(),
    ))?;
    if response.success {
        let now = chrono::Utc::now().timestamp();
        let streak = account.checkin_streak.unwrap_or(0).saturating_add(1);
        cockpit_core::modules::workbuddy_account::update_checkin_info(
            &account_id,
            Some(now),
            streak,
            response.reward.clone(),
        )?;
        publish_event(
            "workbuddy:checkin_completed",
            json!({
                "accountId": account_id,
                "success": true,
                "reward": response.reward.clone(),
                "streak": streak
            }),
        );
    }
    to_value_result(Ok::<_, String>(response))
}

fn handle_deferred_core_method(method: &str, params: &Value) -> Result<Option<Value>, String> {
    let value = match method {
        "settings/update/should-check" => to_value_result(should_check_updates()),
        "settings/update/version-jump/check" => to_value_result(check_version_jump()),
        "system/update/linux/install" => to_value_result::<()>(install_linux_update(params)),
        "system/corrupted-file/delete" => to_value_result::<()>(delete_corrupted_file(params)),
        "antigravity/group-settings/display-groups/get" => Ok(get_display_groups()),
        "workbuddy/checkin/status" => get_workbuddy_checkin_status(params),
        "workbuddy/checkin/run" => run_workbuddy_checkin(params),
        "antigravity/runtime/installed-version/get" => {
            to_value_result(get_antigravity_installed_version_info(params))
        }
        "codex/account/refresh-subscription-info" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::codex_quota::refresh_account_subscription_info(
                    &account_id,
                    true,
                ),
            ))
        }
        "codex/instance/threads/sync-all" => {
            to_value_result(cockpit_core::modules::codex_thread_sync::sync_threads_across_instances())
        }
        "codex/instance/sessions/sync-to-instance" => {
            let session_ids = param_string_vec(params, &["sessionIds", "session_ids"])?;
            let target_instance_id =
                param_string(params, &["targetInstanceId", "target_instance_id"])?;
            to_value_result(cockpit_core::modules::codex_thread_sync::sync_sessions_to_instance(
                session_ids,
                target_instance_id,
            ))
        }
        "codex/instance/sessions/visibility/repair" => to_value_result(
            cockpit_core::modules::codex_session_visibility::repair_session_visibility_across_instances(),
        ),
        "codex/instance/sessions/list" => to_value_result(
            cockpit_core::modules::codex_session_manager::list_sessions_across_instances(),
        ),
        "codex/instance/sessions/token-stats" => {
            let session_ids = param_string_vec(params, &["sessionIds", "session_ids"])?;
            to_value_result(
                cockpit_core::modules::codex_session_manager::get_session_token_stats_across_instances(
                    session_ids,
                ),
            )
        }
        "codex/instance/sessions/trash" => {
            let session_ids = param_string_vec(params, &["sessionIds", "session_ids"])?;
            to_value_result(
                cockpit_core::modules::codex_session_manager::move_sessions_to_trash_across_instances(
                    session_ids,
                ),
            )
        }
        "codex/instance/sessions/trash/list" => to_value_result(
            cockpit_core::modules::codex_session_manager::list_trashed_sessions_across_instances(),
        ),
        "codex/instance/sessions/trash/restore" => {
            let session_ids = param_string_vec(params, &["sessionIds", "session_ids"])?;
            to_value_result(
                cockpit_core::modules::codex_session_manager::restore_sessions_from_trash_across_instances(
                    session_ids,
                ),
            )
        }
        "codex/wakeup/cli-status/get" => to_value_result(codex_wakeup_get_cli_status()),
        "codex/wakeup/runtime-config/update" => {
            to_value_result(codex_wakeup_update_runtime_config(params))
        }
        "codex/wakeup/overview/get" => to_value_result(codex_wakeup_get_overview()),
        "codex/wakeup/state/get" => to_value_result(codex_wakeup_get_state()),
        "codex/wakeup/state/save" => to_value_result(codex_wakeup_save_state(params)),
        "codex/wakeup/history/load" => to_value_result(codex_wakeup_load_history()),
        "codex/wakeup/history/clear" => to_value_result(codex_wakeup_clear_history()),
        "codex/wakeup/scope/cancel" => to_value_result(codex_wakeup_cancel_scope(params)),
        "codex/wakeup/scope/release" => to_value_result(codex_wakeup_release_scope(params)),
        "codex/wakeup/test" => to_value_result(codex_wakeup_test(params)),
        "codex/wakeup/task/run" => to_value_result(codex_wakeup_run_task(params)),
        "codex/wakeup/tasks/run-enabled" => {
            to_value_result(codex_wakeup_run_enabled_tasks(params))
        }
        "wakeup/runtime/ensure-ready" => to_value_result(ensure_wakeup_runtime_ready(params)),
        "wakeup/trigger" => to_value_result(wakeup_trigger(params)),
        "wakeup/models/list" => to_value_result(fetch_wakeup_available_models()),
        "wakeup/crontab/validate" => to_value_result(validate_wakeup_crontab(params)),
        "wakeup/state/sync" => to_value_result(sync_wakeup_state(params)),
        "wakeup/tasks/run-enabled" => to_value_result(run_wakeup_enabled_tasks(params)),
        "wakeup/history/load" => to_value_result(load_wakeup_history()),
        "wakeup/history/add" => to_value_result(add_wakeup_history(params)),
        "wakeup/history/clear" => to_value_result(clear_wakeup_history()),
        "wakeup/official-ls-version-mode/set" => {
            to_value_result(set_wakeup_official_ls_version_mode(params))
        }
        "wakeup/scope/cancel" => to_value_result(wakeup_cancel_scope(params)),
        "wakeup/scope/release" => to_value_result(wakeup_release_scope(params)),
        "wakeup/task/confirm" => to_value_result(confirm_wakeup_task(params)),
        "wakeup/task/cancel" => to_value_result(cancel_wakeup_task(params)),
        "wakeup/timeouts/check" => to_value_result(check_wakeup_timeouts()),
        "wakeup/verification/state/load" => to_value_result(wakeup_verification_load_state()),
        "wakeup/verification/history/load" => to_value_result(wakeup_verification_load_history()),
        "wakeup/verification/history/delete" => {
            to_value_result(wakeup_verification_delete_history(params))
        }
        "wakeup/verification/batch/run" => to_value_result(wakeup_verification_run_batch(params)),
        method if method.starts_with("codex/wakeup/") || method.starts_with("wakeup/") => {
            Err(format!("unsupported wakeup RPC method: {method}"))
        }
        _ => return Ok(None),
    }?;
    Ok(Some(value))
}

fn handle_provider_account_method(method: &str, params: &Value) -> Result<Option<Value>, String> {
    let result = match method {
        "provider/current-account/get" => {
            let platform = param_string(params, &["platform"])?;
            to_value_result(get_provider_current_account_id(&platform))
        }
        "cursor/oauth/start" => to_value_result(cockpit_core::modules::cursor_oauth::start_login()),
        "cursor/oauth/complete" => complete_cursor_oauth_login(params),
        "cursor/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::cursor_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "gemini/oauth/start" => {
            to_value_result(block_on(cockpit_core::modules::gemini_oauth::start_login()))
        }
        "gemini/oauth/complete" => complete_gemini_oauth_login(params),
        "gemini/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::gemini_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "gemini/oauth/submit-callback" => {
            let login_id = param_login_id(params)?;
            let callback_url = param_callback_url(params)?;
            to_value_result(cockpit_core::modules::gemini_oauth::submit_callback_url(
                &login_id,
                &callback_url,
            ))
        }
        "github-copilot/oauth/start" => to_value_result(block_on(
            cockpit_core::modules::github_copilot_oauth::start_login(),
        )),
        "github-copilot/oauth/complete" => complete_github_copilot_oauth_login(params),
        "github-copilot/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::github_copilot_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "windsurf/oauth/start" => to_value_result(block_on(
            cockpit_core::modules::windsurf_oauth::start_login(),
        )),
        "windsurf/oauth/complete" => complete_windsurf_oauth_login(params),
        "windsurf/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::windsurf_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "windsurf/oauth/submit-callback" => {
            let login_id = param_login_id(params)?;
            let callback_url = param_callback_url(params)?;
            to_value_result(cockpit_core::modules::windsurf_oauth::submit_callback_url(
                &login_id,
                &callback_url,
            ))
        }
        "kiro/oauth/start" => {
            to_value_result(block_on(cockpit_core::modules::kiro_oauth::start_login()))
        }
        "kiro/oauth/complete" => complete_kiro_oauth_login(params),
        "kiro/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::kiro_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "kiro/oauth/submit-callback" => {
            let login_id = param_login_id(params)?;
            let callback_url = param_callback_url(params)?;
            to_value_result(cockpit_core::modules::kiro_oauth::submit_callback_url(
                &login_id,
                &callback_url,
            ))
        }
        "codebuddy/oauth/start" => to_value_result(block_on(
            cockpit_core::modules::codebuddy_oauth::start_login(),
        )),
        "codebuddy/oauth/complete" => complete_codebuddy_oauth_login(params),
        "codebuddy/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::codebuddy_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "codebuddy-cn/oauth/start" => to_value_result(block_on(
            cockpit_core::modules::codebuddy_cn_oauth::start_login(),
        )),
        "codebuddy-cn/oauth/complete" => complete_codebuddy_cn_oauth_login(params),
        "codebuddy-cn/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::codebuddy_cn_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "workbuddy/oauth/start" => to_value_result(block_on(
            cockpit_core::modules::workbuddy_oauth::start_login(),
        )),
        "workbuddy/oauth/complete" => complete_workbuddy_oauth_login(params),
        "workbuddy/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::workbuddy_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "qoder/oauth/start" => {
            to_value_result(block_on(cockpit_core::modules::qoder_oauth::start_login()))
        }
        "qoder/oauth/peek" => to_value_result(Ok::<_, String>(
            cockpit_core::modules::qoder_oauth::peek_pending_login(),
        )),
        "qoder/oauth/complete" => {
            let login_id = param_login_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::qoder_oauth::complete_login(&login_id),
            ))
        }
        "qoder/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::qoder_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "trae/oauth/start" => {
            to_value_result(block_on(cockpit_core::modules::trae_oauth::start_login()))
        }
        "trae/oauth/complete" => complete_trae_oauth_login(params),
        "trae/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::trae_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "trae/oauth/submit-callback" => {
            let login_id = param_login_id(params)?;
            let callback_url = param_callback_url(params)?;
            to_value_result(cockpit_core::modules::trae_oauth::submit_callback_url(
                &login_id,
                &callback_url,
            ))
        }
        "zed/oauth/start" => {
            to_value_result(block_on(cockpit_core::modules::zed_oauth::start_login()))
        }
        "zed/oauth/peek" => to_value_result(Ok::<_, String>(
            cockpit_core::modules::zed_oauth::peek_pending_login(),
        )),
        "zed/oauth/complete" => {
            let login_id = param_login_id(params)?;
            to_value_result(block_on(cockpit_core::modules::zed_oauth::complete_login(
                &login_id,
            )))
        }
        "zed/oauth/cancel" => {
            let login_id = param_optional_login_id(params)?;
            to_value_result(cockpit_core::modules::zed_oauth::cancel_login(
                login_id.as_deref(),
            ))
        }
        "zed/oauth/submit-callback" => {
            let login_id = param_login_id(params)?;
            let callback_url = param_callback_url(params)?;
            to_value_result(cockpit_core::modules::zed_oauth::submit_callback_url(
                &login_id,
                &callback_url,
            ))
        }
        "announcement/state/get" => to_value_result(block_on(
            cockpit_core::modules::announcement::get_announcement_state_for_version(
                &application_version(),
            ),
        )),
        "announcement/state/force-refresh" => to_value_result(block_on(
            cockpit_core::modules::announcement::force_refresh_announcements_for_version(
                &application_version(),
            ),
        )),
        "announcement/read/mark" => {
            let id = param_string(params, &["id"])?;
            to_value_result(block_on(
                cockpit_core::modules::announcement::mark_announcement_as_read(&id),
            ))
        }
        "announcement/read/mark-all" => to_value_result(block_on(
            cockpit_core::modules::announcement::mark_all_announcements_as_read_for_version(
                &application_version(),
            ),
        )),
        "announcement/top-right-ad/get" => to_value_result(block_on(
            cockpit_core::modules::announcement::get_top_right_ad_state_for_version(
                &application_version(),
            ),
        )),
        "announcement/sponsor-module/get" => to_value_result(block_on(
            cockpit_core::modules::announcement::get_sponsor_module_state_for_version(
                &application_version(),
            ),
        )),
        "announcement/sponsor-module/force-refresh" => to_value_result(block_on(
            cockpit_core::modules::announcement::force_refresh_sponsor_module_for_version(
                &application_version(),
            ),
        )),
        "antigravity/switch-history/load" => {
            to_value_result(cockpit_core::modules::antigravity_switch_history::load_history())
        }
        "antigravity/switch-history/clear" => {
            to_value_result(cockpit_core::modules::antigravity_switch_history::clear_history())
        }
        "antigravity/account/import-old-tools" => to_value_result(block_on(
            cockpit_core::modules::import::import_from_old_tools_logic(),
        )),
        "antigravity/account/import-local" => to_value_result(block_on(
            cockpit_core::modules::import::import_from_local_logic(),
        )),
        "antigravity/account/import-files" => {
            let file_paths = param_string_vec(params, &["filePaths", "file_paths"])?;
            to_value_result(block_on(
                cockpit_core::modules::import::import_from_files_logic(file_paths),
            ))
        }
        "antigravity/account/sync-extension" => to_value_result(block_on(
            cockpit_core::modules::import::import_from_extension_credentials(None),
        )),
        "antigravity/account/sync-current-client" => Ok(Value::Null),
        "antigravity/account-groups/load" => {
            to_value_result(load_data_file("account_groups.json", "[]"))
        }
        "antigravity/account-groups/save" => {
            let data = param_data(params)?;
            to_value_result(save_data_file("account_groups.json", &data))
        }
        "antigravity/group-settings/get" => to_value_result(Ok::<_, String>(
            cockpit_core::modules::group_settings::load_group_settings(),
        )),
        "antigravity/group-settings/save" => save_group_settings_from_params(params),
        "antigravity/group-settings/set-model" => {
            let model_id = param_string(params, &["modelId", "model_id"])?;
            let group_id = param_string(params, &["groupId", "group_id"])?;
            mutate_group_settings(|settings| settings.set_model_group(&model_id, &group_id))
        }
        "antigravity/group-settings/remove-model" => {
            let model_id = param_string(params, &["modelId", "model_id"])?;
            mutate_group_settings(|settings| settings.remove_model_group(&model_id))
        }
        "antigravity/group-settings/set-name" => {
            let group_id = param_string(params, &["groupId", "group_id"])?;
            let name = param_string_or_empty(params, &["name"])?;
            mutate_group_settings(|settings| settings.set_group_name(&group_id, &name))
        }
        "antigravity/group-settings/delete" => {
            let group_id = param_string(params, &["groupId", "group_id"])?;
            mutate_group_settings(|settings| settings.delete_group(&group_id))
        }
        "antigravity/group-settings/update-order" => {
            let order = param_string_vec(params, &["order"])?;
            mutate_group_settings(|settings| settings.set_group_order(order))
        }
        "antigravity/account/add" => add_antigravity_account_from_refresh_token(params),
        "antigravity/account/fetch-quota" => fetch_antigravity_account_quota(params),
        "antigravity/account/refresh-current-quota" => refresh_current_antigravity_quota(),
        "antigravity/account/refresh-all-quotas" => to_value_result(block_on(
            cockpit_core::modules::account::refresh_all_quotas_logic(
                cockpit_core::modules::account::QuotaRefreshTrigger::ManualBatch,
            ),
        )),
        "antigravity/account/switch" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::account::switch_account_local_no_restart(&account_id),
            ))
        }
        "antigravity/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::account::delete_account(&account_id))
        }
        "antigravity/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::account::delete_accounts(
                &account_ids,
            ))
        }
        "antigravity/account/current/get" => {
            to_value_result(cockpit_core::modules::account::get_current_account())
        }
        "antigravity/account/current/set" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::account::set_current_account_id(
                &account_id,
            ))
        }
        "antigravity/account/reorder" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::account::reorder_accounts(
                &account_ids,
            ))
        }
        "antigravity/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(cockpit_core::modules::account::update_account_tags(
                &account_id,
                tags,
            ))
        }
        "antigravity/account/update-notes" => {
            let account_id = param_account_id(params)?;
            let notes = param_string(params, &["notes"])?;
            to_value_result(cockpit_core::modules::account::update_account_notes(
                &account_id,
                notes,
            ))
        }
        "antigravity/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(block_on(
                cockpit_core::modules::import::import_from_json_logic(json_content),
            ))
        }
        "antigravity/account/export" => {
            let account_ids = param_account_ids(params).unwrap_or_default();
            let accounts = if account_ids.is_empty() {
                cockpit_core::modules::account::list_accounts()?
            } else {
                let mut selected = Vec::new();
                for account_id in &account_ids {
                    if let Ok(account) = cockpit_core::modules::account::load_account(account_id) {
                        selected.push(account);
                    }
                }
                selected
            };
            #[derive(Serialize)]
            struct SimpleAccount {
                email: String,
                refresh_token: String,
                #[serde(default, skip_serializing_if = "Vec::is_empty")]
                tags: Vec<String>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                notes: Option<String>,
            }
            let simplified: Vec<SimpleAccount> = accounts
                .into_iter()
                .map(|account| SimpleAccount {
                    email: account.email,
                    refresh_token: account.token.refresh_token,
                    tags: account.tags,
                    notes: account.notes,
                })
                .collect();
            to_value_result(
                serde_json::to_string_pretty(&simplified)
                    .map_err(|err| format!("serialize export failed: {err}")),
            )
        }

        "codex/account/current/get" => to_value_result(Ok::<_, String>(
            cockpit_core::modules::codex_account::get_current_account(),
        )),
        "codex/account/add-token" => add_codex_account_with_token(params),
        "codex/account/add-api-key" => add_codex_account_with_api_key(params),
        "codex/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::codex_account::remove_account(
                &account_id,
            ))
        }
        "codex/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::codex_account::remove_accounts(
                &account_ids,
            ))
        }
        "codex/account/import-local" => to_value_result(
            cockpit_core::modules::codex_account::import_from_local().map(|account| vec![account]),
        ),
        "codex/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(block_on(
                cockpit_core::modules::codex_account::import_from_json(&json_content),
            ))
        }
        "codex/account/import-files" => {
            let file_paths = param_string_vec(params, &["filePaths", "file_paths"])?;
            to_value_result(block_on(
                cockpit_core::modules::codex_account::import_from_files(file_paths),
            ))
        }
        "codex/account/batch-import/start-from-files" => {
            let file_paths = param_string_vec(params, &["filePaths", "file_paths"])?;
            to_value_result(
                cockpit_core::modules::codex_account::start_codex_batch_import_from_files(
                    file_paths,
                ),
            )
        }
        "codex/account/batch-import/cancel" => {
            let session_id = param_string(params, &["sessionId", "session_id"])?;
            to_value_result::<()>(
                cockpit_core::modules::codex_account::cancel_codex_batch_import(&session_id),
            )
        }
        "codex/account/batch-import/resume" => {
            let session_id = param_string(params, &["sessionId", "session_id"])?;
            to_value_result::<()>(
                cockpit_core::modules::codex_account::resume_codex_batch_import(&session_id),
            )
        }
        "codex/account/batch-import/preview" => {
            let session_id = param_string(params, &["sessionId", "session_id"])?;
            to_value_result(
                cockpit_core::modules::codex_account::get_codex_batch_import_preview(&session_id),
            )
        }
        "codex/account/batch-import/confirm" => {
            let session_id = param_string(params, &["sessionId", "session_id"])?;
            let item_ids = param_string_vec(params, &["itemIds", "item_ids"])?;
            to_value_result(
                cockpit_core::modules::codex_account::confirm_codex_batch_import(
                    &session_id,
                    &item_ids,
                ),
            )
        }
        "codex/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::codex_account::export_accounts(
                &account_ids,
            ))
        }
        "codex/account/refresh-profile" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::codex_account::refresh_account_profile(&account_id),
            ))
        }
        "codex/account/refresh-quota" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::codex_quota::refresh_account_quota(&account_id),
            ))
        }
        "codex/account/refresh-current-quota" => refresh_current_codex_quota(),
        "codex/account/refresh-all-quotas" => to_value_result(block_on(async {
            cockpit_core::modules::codex_quota::refresh_all_quotas()
                .await
                .map(count_success)
        })),
        "codex/account/switch" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::codex_account::switch_account_managed(&account_id),
            ))
        }
        "codex/account/app-speed/update" => update_codex_account_app_speed(params),
        "codex/account/update-api-key" => update_codex_api_key_credentials(params),
        "codex/account/update-api-key-bound-oauth" => {
            update_codex_api_key_bound_oauth_account(params)
        }
        "codex/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(cockpit_core::modules::codex_account::update_account_tags(
                &account_id,
                tags,
            ))
        }
        "codex/account/update-note" => {
            let account_id = param_account_id(params)?;
            let note = param_string(params, &["note"])?;
            to_value_result(cockpit_core::modules::codex_account::update_account_note(
                &account_id,
                note,
            ))
        }
        "codex/account/update-name" => {
            let account_id = param_account_id(params)?;
            let name = param_string(params, &["name"])?;
            to_value_result(cockpit_core::modules::codex_account::update_account_name(
                &account_id,
                name,
            ))
        }
        "codex/account-groups/load" => {
            to_value_result(load_data_file("codex_account_groups.json", "[]"))
        }
        "codex/account-groups/save" => {
            let data = param_data(params)?;
            to_value_result(save_data_file("codex_account_groups.json", &data))
        }
        "codex/model-providers/load" => {
            to_value_result(load_data_file("codex_model_providers.json", "[]"))
        }
        "codex/model-providers/save" => {
            let data = param_data(params)?;
            to_value_result(save_data_file("codex_model_providers.json", &data))
        }

        "cursor/account/list" => {
            to_value_result(cockpit_core::modules::cursor_account::list_accounts_checked())
        }
        "cursor/account/add-token" => add_cursor_account_with_token(params),
        "cursor/account/inject" => {
            let account_id = param_account_id(params)?;
            inject_provider_account("cursor", &account_id)
        }
        "cursor/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::cursor_account::remove_account(
                &account_id,
            ))
        }
        "cursor/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::cursor_account::remove_accounts(
                &account_ids,
            ))
        }
        "cursor/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(cockpit_core::modules::cursor_account::import_from_json(
                &json_content,
            ))
        }
        "cursor/account/import-local" => maybe_one(
            cockpit_core::modules::cursor_account::import_from_local(),
            "local Cursor account not found on service host",
        ),
        "cursor/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::cursor_account::export_accounts(
                &account_ids,
            ))
        }
        "cursor/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::cursor_account::refresh_account_async(&account_id),
            ))
        }
        "cursor/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::cursor_account::refresh_all_tokens()
                .await
                .map(count_success)
        })),
        "cursor/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(cockpit_core::modules::cursor_account::update_account_tags(
                &account_id,
                tags,
            ))
        }
        "cursor/account/index-path" => {
            to_value_result(cockpit_core::modules::cursor_account::accounts_index_path_string())
        }

        "gemini/account/list" => {
            to_value_result(cockpit_core::modules::gemini_account::list_accounts_checked())
        }
        "gemini/account/add-token" => add_gemini_account_with_token(params),
        "gemini/account/inject" => {
            let account_id = param_account_id(params)?;
            inject_provider_account("gemini", &account_id)
        }
        "gemini/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::gemini_account::remove_account(
                &account_id,
            ))
        }
        "gemini/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::gemini_account::remove_accounts(
                &account_ids,
            ))
        }
        "gemini/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(cockpit_core::modules::gemini_account::import_from_json(
                &json_content,
            ))
        }
        "gemini/account/import-local" => maybe_one(
            cockpit_core::modules::gemini_account::import_from_local(),
            "local Gemini account not found on service host",
        ),
        "gemini/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::gemini_account::export_accounts(
                &account_ids,
            ))
        }
        "gemini/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::gemini_account::refresh_account_token(&account_id),
            ))
        }
        "gemini/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::gemini_account::refresh_all_tokens()
                .await
                .map(count_success)
        })),
        "gemini/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(cockpit_core::modules::gemini_account::update_account_tags(
                &account_id,
                tags,
            ))
        }
        "gemini/account/index-path" => {
            to_value_result(cockpit_core::modules::gemini_account::accounts_index_path_string())
        }

        "github-copilot/account/list" => {
            to_value_result(cockpit_core::modules::github_copilot_account::list_accounts_checked())
        }
        "github-copilot/account/add-token" => add_github_copilot_account_with_token(params),
        "github-copilot/account/inject" => {
            let account_id = param_account_id(params)?;
            inject_provider_account("github_copilot", &account_id)
        }
        "github-copilot/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(
                cockpit_core::modules::github_copilot_account::remove_account(&account_id),
            )
        }
        "github-copilot/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(
                cockpit_core::modules::github_copilot_account::remove_accounts(&account_ids),
            )
        }
        "github-copilot/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(
                cockpit_core::modules::github_copilot_account::import_from_json(&json_content),
            )
        }
        "github-copilot/account/import-local" => import_github_copilot_from_local(),
        "github-copilot/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(
                cockpit_core::modules::github_copilot_account::export_accounts(&account_ids),
            )
        }
        "github-copilot/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::github_copilot_account::refresh_account_token(&account_id),
            ))
        }
        "github-copilot/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::github_copilot_account::refresh_all_tokens()
                .await
                .map(count_success)
        })),
        "github-copilot/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(
                cockpit_core::modules::github_copilot_account::update_account_tags(
                    &account_id,
                    tags,
                ),
            )
        }
        "github-copilot/account/index-path" => to_value_result(
            cockpit_core::modules::github_copilot_account::accounts_index_path_string(),
        ),

        "windsurf/account/list" => {
            to_value_result(cockpit_core::modules::windsurf_account::list_accounts_checked())
        }
        "windsurf/account/add-token" => add_windsurf_account_with_token(params),
        "windsurf/account/add-password" => add_windsurf_account_with_password(params),
        "windsurf/account/add-password-batch" => add_windsurf_accounts_with_password(params),
        "windsurf/account/inject" => {
            let account_id = param_account_id(params)?;
            inject_provider_account("windsurf", &account_id)
        }
        "windsurf/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::windsurf_account::remove_account(
                &account_id,
            ))
        }
        "windsurf/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::windsurf_account::remove_accounts(
                &account_ids,
            ))
        }
        "windsurf/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(cockpit_core::modules::windsurf_account::import_from_json(
                &json_content,
            ))
        }
        "windsurf/account/import-local" => import_windsurf_from_local(),
        "windsurf/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::windsurf_account::export_accounts(
                &account_ids,
            ))
        }
        "windsurf/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::windsurf_account::refresh_account_token(&account_id),
            ))
        }
        "windsurf/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::windsurf_account::refresh_all_tokens()
                .await
                .map(count_success)
        })),
        "windsurf/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(
                cockpit_core::modules::windsurf_account::update_account_tags(&account_id, tags),
            )
        }
        "windsurf/account/index-path" => {
            to_value_result(cockpit_core::modules::windsurf_account::accounts_index_path_string())
        }

        "kiro/account/list" => {
            to_value_result(cockpit_core::modules::kiro_account::list_accounts_checked())
        }
        "kiro/account/add-token" => add_kiro_account_with_token(params),
        "kiro/account/inject" => {
            let account_id = param_account_id(params)?;
            inject_provider_account("kiro", &account_id)
        }
        "kiro/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::kiro_account::remove_account(
                &account_id,
            ))
        }
        "kiro/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::kiro_account::remove_accounts(
                &account_ids,
            ))
        }
        "kiro/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(cockpit_core::modules::kiro_account::import_from_json(
                &json_content,
            ))
        }
        "kiro/account/import-local" => import_kiro_from_local(),
        "kiro/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::kiro_account::export_accounts(
                &account_ids,
            ))
        }
        "kiro/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::kiro_account::refresh_account_token(&account_id),
            ))
        }
        "kiro/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::kiro_account::refresh_all_tokens()
                .await
                .map(count_success)
        })),
        "kiro/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(cockpit_core::modules::kiro_account::update_account_tags(
                &account_id,
                tags,
            ))
        }
        "kiro/account/index-path" => {
            to_value_result(cockpit_core::modules::kiro_account::accounts_index_path_string())
        }

        "codebuddy/account/list" => {
            to_value_result(cockpit_core::modules::codebuddy_account::list_accounts_checked())
        }
        "codebuddy/account/add-token" => add_codebuddy_account_with_token(params),
        "codebuddy/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::codebuddy_account::remove_account(
                &account_id,
            ))
        }
        "codebuddy/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::codebuddy_account::remove_accounts(
                &account_ids,
            ))
        }
        "codebuddy/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(cockpit_core::modules::codebuddy_account::import_from_json(
                &json_content,
            ))
        }
        "codebuddy/account/import-local" => import_codebuddy_from_local(),
        "codebuddy/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::codebuddy_account::export_accounts(
                &account_ids,
            ))
        }
        "codebuddy/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::codebuddy_account::refresh_account_token(&account_id),
            ))
        }
        "codebuddy/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::codebuddy_account::refresh_all_tokens()
                .await
                .map(count_success)
        })),
        "codebuddy/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(
                cockpit_core::modules::codebuddy_account::update_account_tags(&account_id, tags),
            )
        }
        "codebuddy/account/index-path" => {
            to_value_result(cockpit_core::modules::codebuddy_account::accounts_index_path_string())
        }

        "codebuddy-cn/account/list" => {
            to_value_result(cockpit_core::modules::codebuddy_cn_account::list_accounts_checked())
        }
        "codebuddy-cn/account/add-token" => add_codebuddy_cn_account_with_token(params),
        "codebuddy-cn/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::codebuddy_cn_account::remove_account(
                &account_id,
            ))
        }
        "codebuddy-cn/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(
                cockpit_core::modules::codebuddy_cn_account::remove_accounts(&account_ids),
            )
        }
        "codebuddy-cn/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(
                cockpit_core::modules::codebuddy_cn_account::import_from_json(&json_content),
            )
        }
        "codebuddy-cn/account/import-local" => import_codebuddy_cn_from_local(),
        "codebuddy-cn/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(
                cockpit_core::modules::codebuddy_cn_account::export_accounts(&account_ids),
            )
        }
        "codebuddy-cn/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::codebuddy_cn_account::refresh_account_token(&account_id),
            ))
        }
        "codebuddy-cn/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::codebuddy_cn_account::refresh_all_tokens()
                .await
                .map(count_success)
        })),
        "codebuddy-cn/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(
                cockpit_core::modules::codebuddy_cn_account::update_account_tags(&account_id, tags),
            )
        }
        "codebuddy-cn/account/sync-to-workbuddy" => to_value_result(
            cockpit_core::modules::codebuddy_cn_account::sync_accounts_to_workbuddy()
                .map(|count| count as i32),
        ),
        "codebuddy-cn/account/index-path" => to_value_result(
            cockpit_core::modules::codebuddy_cn_account::accounts_index_path_string(),
        ),

        "workbuddy/account/list" => {
            to_value_result(cockpit_core::modules::workbuddy_account::list_accounts_checked())
        }
        "workbuddy/account/add-token" => add_workbuddy_account_with_token(params),
        "workbuddy/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::workbuddy_account::remove_account(
                &account_id,
            ))
        }
        "workbuddy/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::workbuddy_account::remove_accounts(
                &account_ids,
            ))
        }
        "workbuddy/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(cockpit_core::modules::workbuddy_account::import_from_json(
                &json_content,
            ))
        }
        "workbuddy/account/import-local" => import_workbuddy_from_local(),
        "workbuddy/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::workbuddy_account::export_accounts(
                &account_ids,
            ))
        }
        "workbuddy/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::workbuddy_account::refresh_account_token(&account_id),
            ))
        }
        "workbuddy/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::workbuddy_account::refresh_all_tokens()
                .await
                .map(count_success)
        })),
        "workbuddy/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(
                cockpit_core::modules::workbuddy_account::update_account_tags(&account_id, tags),
            )
        }
        "workbuddy/account/sync-to-codebuddy-cn" => to_value_result(
            cockpit_core::modules::workbuddy_account::sync_accounts_to_codebuddy_cn()
                .map(|count| count as i32),
        ),
        "workbuddy/account/index-path" => {
            to_value_result(cockpit_core::modules::workbuddy_account::accounts_index_path_string())
        }

        "qoder/account/list" => {
            to_value_result(cockpit_core::modules::qoder_account::list_accounts_checked())
        }
        "qoder/account/inject" => {
            let account_id = param_account_id(params)?;
            inject_provider_account("qoder", &account_id)
        }
        "qoder/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::qoder_account::remove_account(
                &account_id,
            ))
        }
        "qoder/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::qoder_account::remove_accounts(
                &account_ids,
            ))
        }
        "qoder/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(cockpit_core::modules::qoder_account::import_from_json(
                &json_content,
            ))
        }
        "qoder/account/import-local" => maybe_one(
            cockpit_core::modules::qoder_account::import_from_local(),
            "local Qoder account not found on service host",
        ),
        "qoder/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::qoder_account::export_accounts(
                &account_ids,
            ))
        }
        "qoder/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::qoder_oauth::refresh_account_from_openapi(&account_id),
            ))
        }
        "qoder/account/refresh-all" => to_value_result(block_on(
            cockpit_core::modules::qoder_oauth::refresh_all_accounts_from_openapi(),
        )),
        "qoder/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(cockpit_core::modules::qoder_account::update_account_tags(
                &account_id,
                tags,
            ))
        }
        "qoder/account/index-path" => {
            to_value_result(cockpit_core::modules::qoder_account::accounts_index_path_string())
        }

        "trae/account/list" => {
            to_value_result(cockpit_core::modules::trae_account::list_accounts_checked())
        }
        "trae/account/add-token" => add_trae_account_with_token(params),
        "trae/account/inject" => {
            let account_id = param_account_id(params)?;
            inject_provider_account("trae", &account_id)
        }
        "trae/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::trae_account::remove_account(
                &account_id,
            ))
        }
        "trae/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::trae_account::remove_accounts(
                &account_ids,
            ))
        }
        "trae/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(cockpit_core::modules::trae_account::import_from_json(
                &json_content,
            ))
        }
        "trae/account/import-local" => maybe_one(
            cockpit_core::modules::trae_account::import_from_local(),
            "local Trae account not found on service host",
        ),
        "trae/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::trae_account::export_accounts(
                &account_ids,
            ))
        }
        "trae/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::trae_account::refresh_account_async(&account_id),
            ))
        }
        "trae/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::trae_account::refresh_all_tokens()
                .await
                .map(count_success)
        })),
        "trae/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(cockpit_core::modules::trae_account::update_account_tags(
                &account_id,
                tags,
            ))
        }
        "trae/account/index-path" => {
            to_value_result(cockpit_core::modules::trae_account::accounts_index_path_string())
        }

        "zed/account/list" => {
            to_value_result(cockpit_core::modules::zed_account::list_accounts_checked())
        }
        "zed/account/inject" => {
            let account_id = param_account_id(params)?;
            inject_provider_account("zed", &account_id)
        }
        "zed/account/logout-current" => {
            cockpit_core::modules::zed_account::clear_current_runtime_account()?;
            let message = match cockpit_core::modules::zed_instance::restart_default_session() {
                Ok(_) => "Logged out current Zed account on service host".to_string(),
                Err(err)
                    if err.starts_with("APP_PATH_NOT_FOUND:")
                        || err.contains("鍚姩 Zed 澶辫触") =>
                {
                    publish_event(
                        "app:path_missing",
                        json!({
                            "app": "zed",
                            "retry": { "kind": "default" }
                        }),
                    );
                    format!(
                        "Logged out current Zed account on service host, but Zed restart failed: {}",
                        err
                    )
                }
                Err(err) => return Err(err),
            };
            to_value_result(Ok::<_, String>(message))
        }
        "zed/runtime/status" => to_value_result(Ok::<_, String>(
            cockpit_core::modules::zed_instance::get_runtime_status(),
        )),
        "zed/runtime/default/start" => {
            to_value_result(cockpit_core::modules::zed_instance::start_default_session())
        }
        "zed/runtime/default/stop" => {
            to_value_result(cockpit_core::modules::zed_instance::stop_default_session())
        }
        "zed/runtime/default/restart" => {
            to_value_result(cockpit_core::modules::zed_instance::restart_default_session())
        }
        "zed/runtime/default/focus" => {
            to_value_result(cockpit_core::modules::zed_instance::focus_default_session())
        }
        "zed/account/delete" => {
            let account_id = param_account_id(params)?;
            to_value_result(cockpit_core::modules::zed_account::remove_account(
                &account_id,
            ))
        }
        "zed/account/delete-many" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::zed_account::remove_accounts(
                &account_ids,
            ))
        }
        "zed/account/import-json" => {
            let json_content = param_json_content(params)?;
            to_value_result(cockpit_core::modules::zed_account::import_from_json(
                &json_content,
            ))
        }
        "zed/account/import-local" => to_value_result(block_on(async {
            cockpit_core::modules::zed_account::import_from_local()
                .await
                .map(|account| vec![account])
        })),
        "zed/account/export" => {
            let account_ids = param_account_ids(params)?;
            to_value_result(cockpit_core::modules::zed_account::export_accounts(
                &account_ids,
            ))
        }
        "zed/account/refresh" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(
                cockpit_core::modules::zed_account::refresh_account(&account_id),
            ))
        }
        "zed/account/refresh-all" => to_value_result(block_on(async {
            cockpit_core::modules::zed_account::refresh_all_accounts()
                .await
                .map(|accounts| accounts.len() as i32)
        })),
        "zed/account/update-tags" => {
            let account_id = param_account_id(params)?;
            let tags = param_tags(params)?;
            to_value_result(cockpit_core::modules::zed_account::update_account_tags(
                &account_id,
                tags,
            ))
        }

        _ => return Ok(None),
    };
    result.map(Some)
}

fn codex_local_access_activate(
) -> Result<cockpit_core::models::codex_local_access::CodexLocalAccessState, String> {
    block_on(async {
        let codex_home = cockpit_core::modules::codex_account::get_codex_home();
        let state =
            cockpit_core::modules::codex_local_access::activate_local_access_for_dir(&codex_home)
                .await?;
        let api_service_speed =
            cockpit_core::modules::codex_speed::get_api_service_app_speed_config()?.speed;
        cockpit_core::modules::codex_speed::write_official_app_speed(api_service_speed.clone())?;

        let mut index = cockpit_core::modules::codex_account::load_account_index();
        index.current_account_id = None;
        cockpit_core::modules::codex_account::save_account_index(&index)?;

        if let Err(err) = cockpit_core::modules::codex_instance::update_default_settings(
            Some(Some(
                cockpit_core::modules::codex_instance::CODEX_API_SERVICE_BIND_ACCOUNT_ID
                    .to_string(),
            )),
            None,
            Some(false),
            None,
        ) {
            cockpit_core::modules::logger::log_warn(&format!(
                "update Codex default instance for API service mode failed: {err}"
            ));
        }
        if let Err(err) =
            cockpit_core::modules::codex_instance::update_default_app_speed(api_service_speed)
        {
            cockpit_core::modules::logger::log_warn(&format!(
                "update Codex default instance API service speed failed: {err}"
            ));
        }

        Ok(state)
    })
}

fn handle_codex_local_access_method(method: &str, params: &Value) -> Result<Option<Value>, String> {
    use cockpit_core::models::codex_local_access as model;
    use cockpit_core::modules::codex_local_access as local_access;

    let result = match method {
        "codex/local-access/get-state" => {
            to_value_result(block_on(local_access::get_local_access_state()))
        }
        "codex/local-access/save-accounts" => {
            let account_ids = param_account_ids(params)?;
            let restrict_free_accounts =
                param_optional_bool(params, &["restrictFreeAccounts", "restrict_free_accounts"])?
                    .unwrap_or(true);
            to_value_result(block_on(local_access::save_local_access_accounts(
                account_ids,
                restrict_free_accounts,
            )))
        }
        "codex/local-access/remove-account" => {
            let account_id = param_account_id(params)?;
            to_value_result(block_on(local_access::remove_local_access_account(
                &account_id,
            )))
        }
        "codex/local-access/rotate-api-key" => {
            to_value_result(block_on(local_access::rotate_local_access_api_key()))
        }
        "codex/local-access/update-bound-oauth-account" => {
            let bound_oauth_account_id =
                param_optional_string(params, &["boundOAuthAccountId", "bound_oauth_account_id"])?;
            to_value_result(block_on(
                local_access::update_local_access_bound_oauth_account(bound_oauth_account_id),
            ))
        }
        "codex/local-access/clear-stats" => {
            to_value_result(block_on(local_access::clear_local_access_stats()))
        }
        "codex/local-access/query-request-logs" => {
            let page = param_u32(params, &["page"])?;
            let page_size = param_u32(params, &["pageSize", "page_size"])?;
            let stats_range = param_optional_string(params, &["statsRange", "stats_range"])?;
            let model_query = param_optional_string(params, &["modelQuery", "model_query"])?;
            let account_query = param_optional_string(params, &["accountQuery", "account_query"])?;
            let api_key_query = param_optional_string(params, &["apiKeyQuery", "api_key_query"])?;
            let gateway_mode = param_optional_json::<model::CodexLocalAccessGatewayMode>(
                params,
                &["gatewayMode", "gateway_mode"],
            )?;
            let request_kind = param_optional_json::<model::CodexLocalAccessRequestKind>(
                params,
                &["requestKind", "request_kind"],
            )?;
            let success = param_optional_bool(params, &["success"])?;
            let error_category =
                param_optional_string(params, &["errorCategory", "error_category"])?;
            to_value_result(block_on(local_access::query_local_access_usage_events(
                page,
                page_size,
                stats_range,
                model_query,
                account_query,
                api_key_query,
                gateway_mode,
                request_kind,
                success,
                error_category,
            )))
        }
        "codex/local-access/prepare-restart" => to_value_result(block_on(
            local_access::prepare_local_access_gateway_for_restart(),
        )),
        "codex/local-access/kill-port" => {
            to_value_result(block_on(local_access::kill_local_access_port_processes()))
        }
        "codex/local-access/update-port" => {
            let port = param_u16(params, &["port"])?;
            to_value_result(block_on(local_access::update_local_access_port(port)))
        }
        "codex/local-access/update-routing-strategy" => {
            let strategy =
                param_json::<model::CodexLocalAccessRoutingStrategy>(params, &["strategy"])?;
            to_value_result(block_on(
                local_access::update_local_access_routing_strategy(strategy),
            ))
        }
        "codex/local-access/update-custom-routing" => {
            let rules =
                param_json::<Vec<model::CodexLocalAccessCustomRoutingRule>>(params, &["rules"])?;
            to_value_result(block_on(local_access::update_local_access_custom_routing(
                rules,
            )))
        }
        "codex/local-access/update-account-model-rules" => {
            let rules =
                param_json::<Vec<model::CodexLocalAccessAccountModelRule>>(params, &["rules"])?;
            to_value_result(block_on(
                local_access::update_local_access_account_model_rules(rules),
            ))
        }
        "codex/local-access/update-model-rules" => {
            let model_aliases = param_json::<Vec<model::CodexLocalAccessModelAlias>>(
                params,
                &["modelAliases", "model_aliases"],
            )?;
            let excluded_models = param_string_vec(params, &["excludedModels", "excluded_models"])?;
            to_value_result(block_on(local_access::update_local_access_model_rules(
                model_aliases,
                excluded_models,
            )))
        }
        "codex/local-access/update-model-pricings" => {
            let model_pricings = param_json::<Vec<model::CodexLocalAccessModelPricing>>(
                params,
                &["modelPricings", "model_pricings"],
            )?;
            to_value_result(block_on(local_access::update_local_access_model_pricings(
                model_pricings,
            )))
        }
        "codex/local-access/update-routing-options" => {
            let session_affinity = param_bool(params, &["sessionAffinity", "session_affinity"])?;
            let session_affinity_ttl_ms =
                param_optional_i64(params, &["sessionAffinityTtlMs", "session_affinity_ttl_ms"])?
                    .ok_or_else(|| "missing integer param: sessionAffinityTtlMs".to_string())?;
            let max_retry_credentials =
                param_u16(params, &["maxRetryCredentials", "max_retry_credentials"])?;
            let max_retry_interval_ms =
                param_u64(params, &["maxRetryIntervalMs", "max_retry_interval_ms"])?;
            let disable_cooling = param_bool(params, &["disableCooling", "disable_cooling"])?;
            to_value_result(block_on(local_access::update_local_access_routing_options(
                session_affinity,
                session_affinity_ttl_ms,
                max_retry_credentials,
                max_retry_interval_ms,
                disable_cooling,
            )))
        }
        "codex/local-access/update-timeouts" => {
            let timeouts = param_json::<model::CodexLocalAccessTimeouts>(params, &["timeouts"])?;
            let active_timeout_preset_id = param_optional_string(
                params,
                &["activeTimeoutPresetId", "active_timeout_preset_id"],
            )?;
            to_value_result(block_on(local_access::update_local_access_timeouts(
                timeouts,
                active_timeout_preset_id,
            )))
        }
        "codex/local-access/update-timeout-presets" => {
            let timeout_presets = param_json::<Vec<model::CodexLocalAccessTimeoutPreset>>(
                params,
                &["timeoutPresets", "timeout_presets"],
            )?;
            let active_timeout_preset_id = param_optional_string(
                params,
                &["activeTimeoutPresetId", "active_timeout_preset_id"],
            )?;
            to_value_result(block_on(local_access::update_local_access_timeout_presets(
                timeout_presets,
                active_timeout_preset_id,
            )))
        }
        "codex/local-access/update-upstream-proxy-config" => {
            let upstream_proxy_url =
                param_optional_string(params, &["upstreamProxyUrl", "upstream_proxy_url"])?;
            to_value_result(block_on(
                local_access::update_local_access_upstream_proxy_config(upstream_proxy_url),
            ))
        }
        "codex/local-access/update-gateway-mode" => {
            let gateway_mode =
                param_json::<model::CodexLocalAccessGatewayMode>(params, &["gatewayMode"])?;
            to_value_result(block_on(local_access::update_local_access_gateway_mode(
                gateway_mode,
            )))
        }
        "codex/local-access/update-debug-logs" => {
            let debug_logs = param_bool(params, &["debugLogs", "debug_logs"])?;
            to_value_result(block_on(local_access::update_local_access_debug_logs(
                debug_logs,
            )))
        }
        "codex/local-access/update-access-scope" => {
            let access_scope =
                param_json::<model::CodexLocalAccessScope>(params, &["accessScope"])?;
            to_value_result(block_on(local_access::update_local_access_scope(
                access_scope,
            )))
        }
        "codex/local-access/update-client-base-url-host" => {
            let client_base_url_host = param_json::<model::CodexLocalAccessClientBaseUrlHost>(
                params,
                &["clientBaseUrlHost"],
            )?;
            to_value_result(block_on(
                local_access::update_local_access_client_base_url_host(client_base_url_host),
            ))
        }
        "codex/local-access/update-image-generation-mode" => {
            let image_generation_mode = param_json::<model::CodexLocalAccessImageGenerationMode>(
                params,
                &["imageGenerationMode"],
            )?;
            to_value_result(block_on(
                local_access::update_local_access_image_generation_mode(image_generation_mode),
            ))
        }
        "codex/local-access/create-api-key" => {
            let label = param_optional_string(params, &["label"])?;
            to_value_result(block_on(local_access::create_local_access_api_key(label)))
        }
        "codex/local-access/update-api-key" => {
            let api_key_id = param_string(params, &["apiKeyId", "api_key_id"])?;
            let label = param_optional_string(params, &["label"])?;
            let enabled = param_optional_bool(params, &["enabled"])?;
            let model_prefix = param_optional_string(params, &["modelPrefix", "model_prefix"])?;
            let allowed_models = param_optional_string_vec(params, &["allowedModels"])?;
            let excluded_models = param_optional_string_vec(params, &["excludedModels"])?;
            to_value_result(block_on(local_access::update_local_access_api_key(
                api_key_id,
                label,
                enabled,
                model_prefix,
                allowed_models,
                excluded_models,
            )))
        }
        "codex/local-access/rotate-named-api-key" => {
            let api_key_id = param_string(params, &["apiKeyId", "api_key_id"])?;
            to_value_result(block_on(local_access::rotate_local_access_named_api_key(
                api_key_id,
            )))
        }
        "codex/local-access/delete-api-key" => {
            let api_key_id = param_string(params, &["apiKeyId", "api_key_id"])?;
            to_value_result(block_on(local_access::delete_local_access_api_key(
                api_key_id,
            )))
        }
        "codex/local-access/set-enabled" => {
            let enabled = param_bool(params, &["enabled"])?;
            to_value_result(block_on(local_access::set_local_access_enabled(enabled)))
        }
        "codex/local-access/activate" => to_value_result(codex_local_access_activate()),
        "codex/local-access/test" => {
            to_value_result(block_on(local_access::test_local_access_with_dialog()))
        }
        "codex/local-access/chat-test" => {
            let model_id = param_string(params, &["modelId", "model_id"])?;
            let messages =
                param_json::<Vec<model::CodexLocalAccessChatMessage>>(params, &["messages"])?;
            to_value_result(block_on(local_access::chat_local_access_with_dialog(
                model_id, messages,
            )))
        }
        "codex/local-access/chat-test-stream" => {
            let session_id = param_string(params, &["sessionId", "session_id"])?;
            let model_id = param_string(params, &["modelId", "model_id"])?;
            let messages =
                param_json::<Vec<model::CodexLocalAccessChatMessage>>(params, &["messages"])?;
            to_value_result(block_on(
                local_access::stream_chat_local_access_with_dialog(session_id, model_id, messages),
            ))
        }
        _ => return Ok(None),
    };

    result.map(Some)
}

fn update_runtime_info() -> Value {
    serde_json::to_value(cockpit_core::modules::linux_updater::get_update_runtime_info())
        .unwrap_or(Value::Null)
}

fn rpc_event_name(method: &str) -> Option<&'static str> {
    if method.starts_with("settings/") {
        return Some("settings:changed");
    }
    if method == "system/wakeup-override/set" {
        return Some("wakeup:changed");
    }
    if matches!(
        method,
        "system/app-path/set"
            | "system/app-path/detect"
            | "codex/launch-on-switch/set"
            | "codex/local-access-entry-visible/set"
            | "desktop-shell/window/close"
            | "desktop-shell/floating-card/always-on-top/set"
            | "desktop-shell/floating-card/confirm-on-close/set"
            | "desktop-shell/floating-card/position/save"
            | "desktop-shell/tray-layout/save"
            | "backup/auto/settings/save"
            | "backup/auto/last-run/update"
            | "data-transfer/user-config/apply"
            | "webdav/settings/save"
            | "webdav/backup/upload-auto"
            | "webdav/backup/file/read"
            | "webdav/backup/file/delete"
    ) {
        return Some("settings:changed");
    }
    if method.starts_with("webdav/") && !method.ends_with("/get") && !method.ends_with("/list") {
        return Some("webdav:changed");
    }
    if method == "data-transfer/instance-store/replace" {
        return Some("settings:changed");
    }
    if method.starts_with("backup/auto/") {
        let readonly =
            method.ends_with("/get") || method.ends_with("/list") || method.ends_with("/read");
        if !readonly {
            return Some("auto-backup:changed");
        }
    }
    if method.contains("/group-settings/") || method.contains("/account-groups/") {
        return Some("accounts:changed");
    }
    if method.contains("/oauth/") {
        return Some("oauth:changed");
    }
    if method.contains("/local-access/") {
        return Some("codex-local-access:changed");
    }
    if method.contains("/account/") {
        let readonly = method.ends_with("/list")
            || method.ends_with("/export")
            || method.ends_with("/index-path")
            || method.ends_with("/current/get")
            || method.ends_with("/fetch-quota");
        if !readonly {
            return Some("accounts:changed");
        }
    }
    None
}

pub fn handle_request(req: JsonRpcRequest, service_addr: &str) -> JsonRpcResponse {
    let id = req.id.clone();
    let requested_method = req.method.clone();
    let method = rpc_catalog::canonical_method(&requested_method);
    let response = match method.as_ref() {
        "health" => ok(id, json!({ "ok": true })),
        "initialize" => ok(
            id,
            serde_json::to_value(InitializeResult {
                version: application_version(),
                service_addr: service_addr.to_string(),
                platform_family: std::env::consts::FAMILY,
                platform_os: std::env::consts::OS,
            })
            .unwrap_or(Value::Null),
        ),
        "antigravity/account/list" => {
            value_or_rpc_error(id, cockpit_core::modules::account::list_accounts())
        }
        "antigravity/oauth/prepare-url" => {
            value_or_rpc_error(id, prepare_antigravity_oauth_url(service_addr))
        }
        "antigravity/oauth/start" => {
            value_or_rpc_error(id, start_antigravity_oauth_login(service_addr))
        }
        "antigravity/oauth/complete" => value_or_rpc_error(id, complete_antigravity_oauth_login()),
        "antigravity/oauth/submit-callback" => {
            value_or_rpc_error::<()>(id, submit_antigravity_oauth_callback_url(&req.params))
        }
        "antigravity/oauth/cancel" => {
            value_or_rpc_error::<()>(id, cancel_antigravity_oauth_login())
        }
        "codex/account/list" => value_or_rpc_error(
            id,
            cockpit_core::modules::codex_account::list_accounts_checked(),
        ),
        "codex/oauth/start" => value_or_rpc_error(id, codex_oauth_login_start()),
        "codex/oauth/complete" => value_or_rpc_error(id, codex_oauth_login_completed(&req.params)),
        "codex/oauth/cancel" => value_or_rpc_error::<()>(id, codex_oauth_login_cancel(&req.params)),
        "codex/oauth/submit-callback" => {
            value_or_rpc_error::<()>(id, codex_oauth_submit_callback_url(&req.params))
        }
        "codex/oauth/port/in-use" => value_or_rpc_error(id, is_codex_oauth_port_in_use()),
        "codex/oauth/port/close" => value_or_rpc_error(id, close_codex_oauth_port()),
        "settings/general/get" => ok(
            id,
            serde_json::to_value(cockpit_core::modules::config::get_general_config(None))
                .unwrap_or(Value::Null),
        ),
        "settings/general/save" => value_or_rpc_error::<()>(
            id,
            cockpit_core::modules::config::save_general_config_patch(&req.params),
        ),
        "settings/network/get" => value_or_rpc_error(id, get_network_config()),
        "settings/network/save" => value_or_rpc_error(id, save_network_config(&req.params)),
        "settings/update/get" => value_or_rpc_error(id, load_update_settings()),
        "settings/update/save" => {
            let payload = req
                .params
                .as_object()
                .and_then(|params| params.get("settings"))
                .cloned()
                .unwrap_or_else(|| req.params.clone());
            let result = serde_json::from_value::<
                cockpit_core::modules::update_checker::UpdateSettings,
            >(payload)
            .map_err(|err| format!("invalid update settings payload: {err}"))
            .and_then(|settings| save_update_settings(&settings));
            value_or_rpc_error::<()>(id, result)
        }
        "settings/update/last-check/update" => {
            value_or_rpc_error::<()>(id, update_last_check_time())
        }
        "settings/update/pending-notes/save" => {
            value_or_rpc_error::<()>(id, save_pending_update_notes(&req.params))
        }
        "settings/update/release-history/get" => match get_release_history(&req.params) {
            Ok(value) => ok(id, value),
            Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
        },
        "system/log/update" => value_or_rpc_error::<()>(id, update_log(&req.params)),
        "system/update/runtime-info" => ok(id, update_runtime_info()),
        "system/terminal/list" => ok(
            id,
            json!(cockpit_core::modules::system_host::available_terminals()),
        ),
        "system/downloads-dir/get" => value_or_rpc_error(id, downloads_dir()),
        "system/home-dir/get" => value_or_rpc_error(id, home_dir()),
        "system/text-file/save" => value_or_rpc_error::<()>(id, save_text_file(&req.params)),
        "system/text-file/read" => value_or_rpc_error(id, read_text_file(&req.params)),
        "system/data-folder/open" => value_or_rpc_error::<()>(id, open_data_folder()),
        "system/folder/open" => value_or_rpc_error::<()>(id, open_folder(&req.params)),
        "system/path/open" => value_or_rpc_error::<()>(id, open_system_path(&req.params)),
        "system/app-path/set" => value_or_rpc_error::<()>(id, set_app_path(&req.params)),
        "system/app-path/detect" => value_or_rpc_error(id, detect_app_path(&req.params)),
        "system/wakeup-override/set" => {
            value_or_rpc_error::<()>(id, set_wakeup_override(&req.params))
        }
        "external-import/pending/take" => ok(
            id,
            serde_json::to_value(
                cockpit_core::modules::external_import::take_pending_external_import(),
            )
            .unwrap_or(Value::Null),
        ),
        "external-import/pending/submit-url" => {
            value_or_rpc_error(id, external_import_submit_url(&req.params))
        }
        "external-import/fetch-url" => {
            value_or_rpc_error(id, external_import_fetch_import_url(&req.params))
        }
        "desktop-shell/window/close" => {
            value_or_rpc_error::<()>(id, handle_desktop_shell_window_close(&req.params))
        }
        "desktop-shell/floating-card/show" => ok(id, Value::Null),
        "desktop-shell/floating-card/show-instance" => ok(id, Value::Null),
        "desktop-shell/floating-card/context/get" => ok(id, Value::Null),
        "desktop-shell/floating-card/hide" => ok(id, Value::Null),
        "desktop-shell/floating-card/hide-current" => ok(id, Value::Null),
        "desktop-shell/floating-card/always-on-top/set" => {
            value_or_rpc_error::<()>(id, set_floating_card_always_on_top(&req.params))
        }
        "desktop-shell/floating-card/current-always-on-top/set" => ok(id, Value::Null),
        "desktop-shell/floating-card/confirm-on-close/set" => {
            value_or_rpc_error::<()>(id, set_floating_card_confirm_on_close(&req.params))
        }
        "desktop-shell/floating-card/position/save" => {
            value_or_rpc_error::<()>(id, save_floating_card_position(&req.params))
        }
        "desktop-shell/main-window/navigate" => ok(id, Value::Null),
        "desktop-shell/tray-layout/save" => {
            value_or_rpc_error(id, save_tray_platform_layout(&req.params))
        }
        "logs/snapshot/get" => value_or_rpc_error(id, logs_get_snapshot(&req.params)),
        "logs/directory/open" => value_or_rpc_error::<()>(id, logs_open_log_directory()),
        "backup/auto/settings/get" => value_or_rpc_error(id, get_auto_backup_settings()),
        "backup/auto/settings/save" => {
            value_or_rpc_error(id, save_auto_backup_settings(&req.params))
        }
        "backup/auto/last-run/update" => {
            value_or_rpc_error(id, update_auto_backup_last_run(&req.params))
        }
        "backup/auto/file/write" => value_or_rpc_error(id, write_auto_backup_file(&req.params)),
        "backup/auto/file/read" => value_or_rpc_error(id, read_auto_backup_file(&req.params)),
        "backup/auto/file/copy" => value_or_rpc_error(id, copy_auto_backup_file(&req.params)),
        "backup/auto/file/delete" => {
            value_or_rpc_error::<()>(id, delete_auto_backup_file(&req.params))
        }
        "backup/auto/files/list" => value_or_rpc_error(id, list_auto_backup_files()),
        "backup/auto/files/cleanup" => {
            value_or_rpc_error(id, cleanup_auto_backup_files(&req.params))
        }
        "backup/auto/directory/open" => value_or_rpc_error::<()>(id, open_auto_backup_dir()),
        "data-transfer/user-config/get" => ok(
            id,
            serde_json::to_value(cockpit_core::modules::config::get_user_config())
                .unwrap_or(Value::Null),
        ),
        "data-transfer/user-config/apply" => {
            value_or_rpc_error(id, data_transfer_apply_user_config(&req.params))
        }
        "data-transfer/instance-store/get" => {
            value_or_rpc_error(id, data_transfer_get_instance_store(&req.params))
        }
        "data-transfer/instance-store/replace" => {
            value_or_rpc_error::<()>(id, data_transfer_replace_instance_store(&req.params))
        }
        "webdav/settings/get" => ok(
            id,
            serde_json::to_value(get_webdav_sync_settings()).unwrap_or(Value::Null),
        ),
        "webdav/settings/save" => value_or_rpc_error(id, save_webdav_sync_settings(&req.params)),
        "webdav/connection/test" => {
            value_or_rpc_error(id, test_webdav_sync_connection(&req.params))
        }
        "webdav/backup/upload-auto" => {
            value_or_rpc_error(id, upload_auto_backup_to_webdav(&req.params))
        }
        "webdav/backup/files/list" => value_or_rpc_error(id, list_webdav_backup_files()),
        "webdav/backup/file/read" => value_or_rpc_error(id, read_webdav_backup_file(&req.params)),
        "webdav/backup/file/delete" => {
            value_or_rpc_error::<()>(id, delete_webdav_backup_file(&req.params))
        }
        "codex/config-toml/path" => ok(id, json!(codex_config_toml_path())),
        "codex/config-toml/open" => value_or_rpc_error::<()>(id, open_codex_config_toml()),
        "codex/quick-config/get" => value_or_rpc_error(
            id,
            cockpit_core::modules::codex_account::load_current_quick_config(),
        ),
        "codex/quick-config/save" => match save_codex_quick_config(&req.params) {
            Ok(value) => ok(id, value),
            Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
        },
        "codex/app-speed/get" => value_or_rpc_error(
            id,
            cockpit_core::modules::codex_speed::get_app_speed_config(),
        ),
        "codex/app-speed/save" => value_or_rpc_error(
            id,
            param_codex_app_speed(&req.params)
                .and_then(cockpit_core::modules::codex_speed::save_api_service_app_speed),
        ),
        "codex/api-service-app-speed/get" => value_or_rpc_error(
            id,
            cockpit_core::modules::codex_speed::get_api_service_app_speed_config(),
        ),
        "codex/api-service-app-speed/save" => match save_codex_api_service_app_speed(&req.params) {
            Ok(value) => ok(id, value),
            Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
        },
        "codex/model-provider/connection/test" => {
            match test_codex_model_provider_connection(&req.params) {
                Ok(value) => ok(id, value),
                Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
            }
        }
        "codex/model-provider/usage/query" => match query_codex_model_provider_usage(&req.params) {
            Ok(value) => ok(id, value),
            Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
        },
        "codex/instance/defaults/get" => value_or_rpc_error(
            id,
            cockpit_core::modules::codex_instance::get_instance_defaults(),
        ),
        "codex/instance/list" => value_or_rpc_error(id, list_instances_value("codex")),
        "codex/instance/create" => match create_codex_instance(&req.params) {
            Ok(value) => ok(id, codex_instance_result_to_view(value)),
            Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
        },
        "codex/instance/update" => {
            let result =
                param_string(&req.params, &["instanceId", "instance_id"]).and_then(|instance_id| {
                    if instance_id == DEFAULT_INSTANCE_ID {
                        update_default_instance_value("codex", &req.params)
                    } else {
                        update_codex_instance(&req.params).map(codex_instance_result_to_view)
                    }
                });
            match result {
                Ok(value) => ok(id, value),
                Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
            }
        }
        "codex/instance/delete" => {
            let instance_id = param_string(&req.params, &["instanceId", "instance_id"]);
            value_or_rpc_error::<()>(
                id,
                instance_id.and_then(|instance_id| {
                    cockpit_core::modules::codex_instance::delete_instance(&instance_id)
                }),
            )
        }
        "codex/instance/quick-config/get" => match get_codex_instance_quick_config(&req.params) {
            Ok(value) => ok(id, value),
            Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
        },
        "codex/instance/quick-config/save" => match save_codex_instance_quick_config(&req.params) {
            Ok(value) => ok(id, value),
            Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
        },
        "codex/instance/config-toml/open" => {
            value_or_rpc_error::<()>(id, open_codex_instance_config_toml(&req.params))
        }
        "codex/launch-on-switch/set" => {
            value_or_rpc_error::<()>(id, set_codex_launch_on_switch(&req.params))
        }
        "codex/local-access-entry-visible/set" => {
            value_or_rpc_error::<()>(id, set_codex_local_access_entry_visible(&req.params))
        }
        method => match handle_codex_local_access_method(method, &req.params) {
            Ok(Some(value)) => ok(id, value),
            Ok(None) => match handle_instance_method(method, &req.params) {
                Ok(Some(value)) => ok(id, value),
                Ok(None) => match handle_deferred_core_method(method, &req.params) {
                    Ok(Some(value)) => ok(id, value),
                    Ok(None) => match handle_provider_account_method(method, &req.params) {
                        Ok(Some(value)) => ok(id, value),
                        Ok(None) => error(
                            id,
                            -32601,
                            "unknown_method",
                            Some(json!({ "method": requested_method, "resolvedMethod": method })),
                        ),
                        Err(err) => {
                            error(id, -32000, "action_failed", Some(json!({ "message": err })))
                        }
                    },
                    Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
                },
                Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
            },
            Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
        },
    };
    if response.error.is_none() {
        if let Some(event) = rpc_event_name(method.as_ref()) {
            publish_event(
                event,
                json!({
                    "method": method.as_ref(),
                    "requestedMethod": requested_method,
                    "emittedAt": chrono::Utc::now().timestamp_millis()
                }),
            );
        }
    }
    response
}

pub fn handle_json_rpc_body(body: &str, service_addr: &str) -> Result<String, String> {
    let request: JsonRpcRequest =
        serde_json::from_str(body).map_err(|err| format!("invalid json-rpc request: {err}"))?;
    serde_json::to_string(&handle_request(request, service_addr))
        .map_err(|err| format!("serialize json-rpc response failed: {err}"))
}

fn response_json(status: u16, body: impl Into<String>) -> ResponseBox {
    let mut response = Response::from_string(body.into()).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes("content-type", "application/json") {
        response.add_header(header);
    }
    response.boxed()
}

fn response_with_content_type(status: u16, body: Vec<u8>, content_type: &str) -> ResponseBox {
    let mut response = Response::from_data(body).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes("content-type", content_type.as_bytes()) {
        response.add_header(header);
    }
    response.boxed()
}

fn is_loopback(addr: Option<SocketAddr>) -> bool {
    matches!(
        addr.map(|addr| addr.ip()),
        Some(IpAddr::V4(ip)) if ip.is_loopback()
    ) || matches!(
        addr.map(|addr| addr.ip()),
        Some(IpAddr::V6(ip)) if ip.is_loopback()
    )
}

fn request_token_matches(request: &Request) -> bool {
    let expected = rpc_auth_token();
    request.headers().iter().any(|header| {
        header
            .field
            .as_str()
            .as_str()
            .eq_ignore_ascii_case(RPC_TOKEN_HEADER)
            && header.value.as_str() == expected
    })
}

fn read_body(request: &mut Request) -> Result<String, String> {
    let mut body = String::new();
    request
        .as_reader()
        .take(2 * 1024 * 1024)
        .read_to_string(&mut body)
        .map_err(|err| format!("read request body failed: {err}"))?;
    Ok(body)
}

fn make_header(name: &str, value: &str) -> Option<Header> {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

fn sse_frame(event: &str, data: Value) -> Vec<u8> {
    let payload = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    let mut frame = format!("event: {event}\ndata: {payload}\n\n").into_bytes();
    if frame.len() < SSE_FLUSH_PADDING_BYTES {
        frame.extend_from_slice(b":");
        frame.extend(std::iter::repeat(b' ').take(SSE_FLUSH_PADDING_BYTES - frame.len()));
        frame.extend_from_slice(b"\n\n");
    }
    frame
}

pub fn publish_event(event: &str, data: Value) {
    let frame = sse_frame(event, data);
    if let Ok(mut subscribers) = EVENT_SUBSCRIBERS.lock() {
        subscribers.retain(|subscriber| subscriber.send(frame.clone()).is_ok());
    }
}

fn install_core_event_publishers() {
    cockpit_core::modules::codex_local_access::set_local_access_event_publisher(Some(Arc::new(
        |event, payload| publish_event(event, payload),
    )));
    cockpit_core::modules::codex_account::set_codex_batch_import_event_publisher(Some(Arc::new(
        |event, payload| publish_event(event, payload),
    )));
}

fn subscribe_events() -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    let _ = sender.send(sse_frame(
        "service.ready",
        json!({
            "emittedAt": chrono::Utc::now().timestamp_millis(),
            "version": env!("CARGO_PKG_VERSION")
        }),
    ));
    if let Ok(mut subscribers) = EVENT_SUBSCRIBERS.lock() {
        subscribers.push(sender.clone());
    }
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(15));
        if sender
            .send(sse_frame(
                "service.heartbeat",
                json!({ "emittedAt": chrono::Utc::now().timestamp_millis() }),
            ))
            .is_err()
        {
            break;
        }
    });
    receiver
}

fn events_response() -> ResponseBox {
    let headers = [
        ("content-type", "text/event-stream; charset=utf-8"),
        ("cache-control", "no-cache"),
        ("x-accel-buffering", "no"),
    ]
    .into_iter()
    .filter_map(|(name, value)| make_header(name, value))
    .collect();
    Response::new(
        StatusCode(200),
        headers,
        EventStreamReader::new(subscribe_events()),
        None,
        None,
    )
    .with_chunked_threshold(0)
    .boxed()
}

fn oauth_html_response(status: u16, title: &str, message: &str) -> ResponseBox {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title></head><body style=\"font-family: system-ui, sans-serif; padding: 48px;\"><h1>{title}</h1><p>{message}</p></body></html>"
    );
    response_with_content_type(status, body.into_bytes(), "text/html; charset=utf-8")
}

fn antigravity_oauth_callback_response(url: &str) -> ResponseBox {
    match submit_antigravity_oauth_callback_raw(url) {
        Ok(()) => oauth_html_response(
            200,
            "Authorization complete",
            "You can close this page and return to Cockpit Tools.",
        ),
        Err(err) => oauth_html_response(400, "Authorization failed", &err),
    }
}

fn handle_http_request(mut request: Request, service_addr: &str) {
    let method = request.method().clone();
    let url = request.url().to_string();
    let response = if method == Method::Get && url.starts_with("/oauth-callback") {
        antigravity_oauth_callback_response(&url)
    } else {
        match (method, url.as_str()) {
            (Method::Get, "/health") => response_json(200, json!({ "ok": true }).to_string()),
            (Method::Get, "/events") => {
                if !request_token_matches(&request) {
                    response_json(401, json!({ "error": "rpc_token_required" }).to_string())
                } else {
                    events_response()
                }
            }
            (Method::Post, "/rpc") => {
                if !is_loopback(request.remote_addr().copied()) && !request_token_matches(&request)
                {
                    response_json(401, json!({ "error": "rpc_token_required" }).to_string())
                } else if !request_token_matches(&request) {
                    response_json(401, json!({ "error": "rpc_token_required" }).to_string())
                } else {
                    match read_body(&mut request)
                        .and_then(|body| handle_json_rpc_body(&body, service_addr))
                    {
                        Ok(body) => response_json(200, body),
                        Err(err) => response_json(400, json!({ "error": err }).to_string()),
                    }
                }
            }
            _ => response_json(404, json!({ "error": "not_found" }).to_string()),
        }
    };
    let _ = request.respond(response);
}

pub fn start_server(addr: &str) -> Result<(), String> {
    install_core_event_publishers();
    let server = Server::http(addr).map_err(|err| format!("bind service {addr} failed: {err}"))?;
    println!("cockpit-service listening on {addr}");
    for request in server.incoming_requests() {
        let service_addr = addr.to_string();
        thread::spawn(move || handle_http_request(request, &service_addr));
    }
    Ok(())
}

pub fn run_from_env() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(resolve_addr_from_env);
    if let Err(err) = start_server(&addr) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_returns_runtime_metadata() {
        let response = handle_request(
            JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: json!(1),
                method: "initialize".to_string(),
                params: Value::Null,
            },
            DEFAULT_ADDR,
        );
        assert!(response.error.is_none());
        assert_eq!(response.result.unwrap()["service_addr"], DEFAULT_ADDR);
    }

    #[test]
    fn unknown_method_returns_jsonrpc_error() {
        let response = handle_request(
            JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: json!("x"),
                method: "missing".to_string(),
                params: Value::Null,
            },
            DEFAULT_ADDR,
        );
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[test]
    fn codex_model_provider_test_allows_empty_api_key() {
        let response = handle_request(
            JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: json!("model-provider"),
                method: "codex_test_model_provider_connection".to_string(),
                params: json!({
                    "baseUrl": "https://example.test/v1",
                    "apiKey": "",
                    "wireApi": "responses"
                }),
            },
            DEFAULT_ADDR,
        );
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        assert_eq!(result["failure"]["title"], "missing_api_key");
        assert_eq!(result["failure"]["cause"], "MISSING_API_KEY");
    }

    #[test]
    fn legacy_codex_instance_list_returns_view_shape() {
        let response = handle_request(
            JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: json!("codex-instances"),
                method: "codex_list_instances".to_string(),
                params: Value::Null,
            },
            DEFAULT_ADDR,
        );
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        let items = result.as_array().expect("instance list should be an array");
        assert!(items.iter().any(|item| item["isDefault"] == true));
        assert!(items.iter().all(|item| item.get("running").is_some()));
    }

    #[test]
    fn legacy_sponsor_module_command_uses_core_announcement_rpc() {
        let response = handle_request(
            JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: json!("sponsor-module"),
                method: "announcement_get_sponsor_module".to_string(),
                params: Value::Null,
            },
            DEFAULT_ADDR,
        );
        assert!(response.error.is_none());
        let result = response
            .result
            .expect("sponsor module should return a result");
        assert!(
            result.get("sponsorModule").is_some(),
            "result should preserve frontend SponsorModuleState shape"
        );
    }

    #[test]
    fn pending_instance_start_is_not_unknown_method() {
        let response = handle_request(
            JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: json!("codex-start"),
                method: "codex_start_instance".to_string(),
                params: json!({ "instanceId": "__default__" }),
            },
            DEFAULT_ADDR,
        );
        let error = response.error.expect("start should be a structured error");
        assert_eq!(error.message, "action_failed");
        let message = error.data.unwrap()["message"].as_str().unwrap().to_string();
        assert!(message.contains("core migration"));
    }

    #[test]
    fn sse_frame_is_padded_to_force_chunk_flush() {
        let frame = sse_frame("service.ready", json!({ "ok": true }));
        assert!(frame.len() > 8192);
        assert!(String::from_utf8_lossy(&frame).starts_with("event: service.ready\ndata:"));
    }
}
