use serde::Serialize;
use serde_json::{json, Value};
use std::future::Future;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default = "default_jsonrpc")]
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
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

fn default_jsonrpc() -> String {
    "2.0".to_string()
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

pub(crate) fn ok(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

pub(crate) fn error(
    id: Value,
    code: i32,
    message: impl Into<String>,
    data: Option<Value>,
) -> JsonRpcResponse {
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

pub(crate) fn value_or_rpc_error<T: Serialize>(
    id: Value,
    result: Result<T, String>,
) -> JsonRpcResponse {
    match result {
        Ok(value) => ok(id, serde_json::to_value(value).unwrap_or(Value::Null)),
        Err(err) => error(id, -32000, "action_failed", Some(json!({ "message": err }))),
    }
}

pub(crate) fn to_value_result<T: Serialize>(result: Result<T, String>) -> Result<Value, String> {
    result.and_then(|value| {
        serde_json::to_value(value).map_err(|err| format!("serialize result failed: {err}"))
    })
}

use std::sync::LazyLock;

/// 全局 runtime handle，由 start_server 初始化
/// block_in_place + Handle::block_on 是安全的：
/// block_in_place 会先创建替代 worker，再阻塞当前线程，不会导致死锁
static RUNTIME_HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();

use std::sync::OnceLock;

pub(crate) fn set_runtime_handle(handle: tokio::runtime::Handle) {
    let _ = RUNTIME_HANDLE.set(handle);
}

fn get_handle() -> tokio::runtime::Handle {
    RUNTIME_HANDLE
        .get()
        .cloned()
        .unwrap_or_else(|| {
            // Fallback: 创建一个临时 runtime（仅测试环境使用）
            static FALLBACK: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("create fallback runtime failed")
            });
            FALLBACK.handle().clone()
        })
}

pub(crate) fn block_on<T, F>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    get_handle().block_on(future)
}

pub(crate) fn block_on_with_timeout<T, F>(
    timeout_ms: u64,
    future: F,
) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    get_handle().block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            future,
        )
        .await
        .map_err(|_| format!("operation timed out after {timeout_ms}ms"))?
    })
}

pub(crate) fn block_on_value<T, F>(future: F) -> Result<T, String>
where
    F: Future<Output = T>,
{
    Ok(get_handle().block_on(future))
}
