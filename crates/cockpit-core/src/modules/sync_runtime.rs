//! 同步运行时桥接模块
//!
//! 提供共享的多线程 tokio runtime，供同步代码调用异步函数。
//! 参考 Codex-Manager 的 `spawn_blocking` 模式，避免 `Handle::block_on` 死锁。

use std::sync::LazyLock;
use std::future::Future;
use std::time::Duration;

/// 共享多线程 tokio runtime
/// 使用独立的 worker 线程池，与宿主 runtime（如 axum/tiny_http）完全隔离
static SHARED_RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("cockpit-core-async")
        .enable_all()
        .build()
        .expect("create cockpit-core shared runtime failed")
});

/// 同步执行异步函数（无超时）
pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    SHARED_RUNTIME.block_on(future)
}

/// 同步执行异步函数（带超时）
pub fn block_on_timeout<T>(timeout_ms: u64, future: impl Future<Output = T>) -> Result<T, String> {
    SHARED_RUNTIME.block_on(async {
        tokio::time::timeout(Duration::from_millis(timeout_ms), future)
            .await
            .map_err(|_| format!("operation timed out after {timeout_ms}ms"))
    })
}
