/// 平台特定逻辑：每个平台的实现各不相同，无法用函数指针表替代
///
/// 这些函数包含平台特有的多步骤编排逻辑。
/// 新增 provider 时需要在此文件中添加对应的 match arm。

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::params::*;
use crate::rpc_types::*;

// === Instance 操作 ===

pub(crate) fn ensure_launch_path_for_platform(platform: &str) -> Result<(), String> {
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

pub(crate) fn resolve_instance_pid_for_platform(
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

pub(crate) fn close_instance_process_for_platform(
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

pub(crate) fn open_instance_window_for_platform(
    platform: &str,
    params: &Value,
    load_default_settings: fn(&str) -> Result<cockpit_core::models::DefaultInstanceSettings, String>,
    find_instance: fn(&str, &str) -> Result<cockpit_core::models::InstanceProfile, String>,
    resolve_pid: fn(&str, Option<u32>, Option<&str>) -> Option<u32>,
) -> Result<Value, String> {
    let instance_id = param_string(params, &["instanceId", "instance_id"])?;
    if platform == "gemini" {
        return Err("Gemini CLI instances do not have a managed app window".to_string());
    }

    let (last_pid, user_data_dir, launch_mode) = if instance_id == "__default__" {
        let settings = load_default_settings(platform)?;
        (settings.last_pid, None, settings.launch_mode)
    } else {
        let instance = find_instance(platform, &instance_id)?;
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
                resolve_pid(platform, last_pid, user_data_dir.as_deref())
                    .ok_or_else(|| "instance is not running".to_string())?;
            cockpit_core::modules::process::focus_process_pid(pid).map(|_| pid)
        }
        other => Err(format!("unsupported instance platform: {other}")),
    }?;
    Ok(Value::Null)
}
