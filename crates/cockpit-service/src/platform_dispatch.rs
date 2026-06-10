/// 平台分发函数：使用 PlatformDef 注册表替代重复的 match 语句
///
/// 新增 provider 不需要修改这些函数，只需在 platform_def.rs 的 PLATFORMS 注册表中添加一行。

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::platform_def::find_platform;

/// 加载平台默认设置
pub(crate) fn load_default_settings_for_platform(
    platform: &str,
) -> Result<cockpit_core::models::DefaultInstanceSettings, String> {
    (find_platform(platform)?.load_default_settings)()
}

/// 更新默认实例 PID
pub(crate) fn update_default_pid_for_platform(
    platform: &str,
    pid: Option<u32>,
) -> Result<cockpit_core::models::DefaultInstanceSettings, String> {
    (find_platform(platform)?.update_default_pid)(pid)
}

/// 更新指定实例 PID
pub(crate) fn update_instance_pid_for_platform(
    platform: &str,
    instance_id: &str,
    pid: Option<u32>,
) -> Result<cockpit_core::models::InstanceProfile, String> {
    (find_platform(platform)?.update_instance_pid)(instance_id, pid)
}

/// 清除所有 PID
pub(crate) fn clear_all_pids_for_platform(platform: &str) -> Result<(), String> {
    (find_platform(platform)?.clear_all_pids)()
}

/// 获取默认用户数据目录
pub(crate) fn instance_default_user_data_dir(platform: &str) -> Result<PathBuf, String> {
    (find_platform(platform)?.get_default_user_data_dir)()
}

/// 获取实例默认值
pub(crate) fn instance_defaults_value(platform: &str) -> Result<Value, String> {
    (find_platform(platform)?.get_instance_defaults)()
}

/// 加载实例存储
pub(crate) fn load_instance_store_for_platform(
    platform: &str,
) -> Result<cockpit_core::models::InstanceStore, String> {
    (find_platform(platform)?.load_instance_store)()
}

/// 删除实例
pub(crate) fn delete_instance_for_platform(
    platform: &str,
    params: &Value,
) -> Result<Value, String> {
    let instance_id = crate::params::param_string(params, &["instanceId", "instance_id"])?;
    (find_platform(platform)?.delete_instance)(&instance_id)?;
    Ok(json!({ "deleted": instance_id }))
}

/// 创建实例
pub(crate) fn create_instance_for_platform(
    platform: &str,
    params: &Value,
) -> Result<Value, String> {
    let profile = (find_platform(platform)?.create_instance_from_json)(params)?;
    serde_json::to_value(profile).map_err(|e| format!("serialize failed: {e}"))
}

/// 更新实例
pub(crate) fn update_instance_for_platform(
    platform: &str,
    params: &Value,
) -> Result<Value, String> {
    let profile = (find_platform(platform)?.update_instance_from_json)(params)?;
    serde_json::to_value(profile).map_err(|e| format!("serialize failed: {e}"))
}

/// 列出所有平台 slug
pub(crate) fn all_platform_slugs() -> Vec<&'static str> {
    crate::platform_def::all_platform_slugs()
}
