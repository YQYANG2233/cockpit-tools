/// PlatformDef: 每个 provider 的统一接口定义
///
/// 新增 provider 只需：
/// 1. 在 PLATFORMS 注册表添加一行
/// 2. 创建 xxx_def() 工厂函数
/// 3. 在 cockpit-core 中实现对应的模块
///
/// 不再需要在 lib.rs 的 14 个 match 语句中逐个添加 arm。

use serde_json::Value;
use std::path::PathBuf;

/// 平台定义：函数指针集合
/// 所有函数指针都指向 cockpit_core::modules 中的对应函数
pub(crate) struct PlatformDef {
    pub slug: &'static str,

    // === Instance 操作（纯委托，12 个平台签名完全一致）===
    pub load_default_settings: fn() -> Result<cockpit_core::models::DefaultInstanceSettings, String>,
    pub update_default_pid: fn(Option<u32>) -> Result<cockpit_core::models::DefaultInstanceSettings, String>,
    pub update_instance_pid: fn(&str, Option<u32>) -> Result<cockpit_core::models::InstanceProfile, String>,
    pub clear_all_pids: fn() -> Result<(), String>,
    pub get_default_user_data_dir: fn() -> Result<PathBuf, String>,
    pub get_instance_defaults: fn() -> Result<Value, String>,
    pub load_instance_store: fn() -> Result<cockpit_core::models::InstanceStore, String>,
    pub delete_instance: fn(&str) -> Result<(), String>,
    // 使用 Value 输入，内部构造 params struct
    pub create_instance_from_json: fn(&Value) -> Result<cockpit_core::models::InstanceProfile, String>,
    pub update_instance_from_json: fn(&Value) -> Result<cockpit_core::models::InstanceProfile, String>,

    // === 能力标记 ===
    pub supports_instance_launch: bool,
}

// === 平台工厂函数 ===
// 每个函数返回一个 PlatformDef，填入对应的 core 模块函数指针

fn antigravity_def() -> PlatformDef {
    PlatformDef {
        slug: "antigravity",
        load_default_settings: || {
            cockpit_core::modules::instance::load_default_settings()
        },
        update_default_pid: |pid| {
            cockpit_core::modules::instance::update_default_pid(pid)
        },
        update_instance_pid: |id, pid| {
            cockpit_core::modules::instance::update_instance_pid(id, pid)
        },
        clear_all_pids: || {
            cockpit_core::modules::instance::clear_all_pids()
        },
        get_default_user_data_dir: || {
            cockpit_core::modules::instance::get_default_user_data_dir()
        },
        get_instance_defaults: || {
            let defaults = cockpit_core::modules::instance::get_instance_defaults()?;
            serde_json::to_value(defaults).map_err(|e| format!("serialize failed: {e}"))
        },
        load_instance_store: || {
            cockpit_core::modules::instance::load_instance_store()
        },
        delete_instance: |id| {
            cockpit_core::modules::instance::delete_instance(id)
        },
        create_instance_from_json: |params| {
            let p = crate::platform_def::parse_create_instance_params(params)?;
            cockpit_core::modules::instance::create_instance(p)
        },
        update_instance_from_json: |params| {
            let p = crate::platform_def::parse_update_instance_params(params)?;
            cockpit_core::modules::instance::update_instance(p)
        },
        supports_instance_launch: true,
    }
}

macro_rules! impl_platform_def {
    ($slug:expr, $mod:ident, $user_data_dir_fn:ident) => {
        PlatformDef {
            slug: $slug,
            load_default_settings: || {
                cockpit_core::modules::$mod::load_default_settings()
            },
            update_default_pid: |pid| {
                cockpit_core::modules::$mod::update_default_pid(pid)
            },
            update_instance_pid: |id, pid| {
                cockpit_core::modules::$mod::update_instance_pid(id, pid)
            },
            clear_all_pids: || {
                cockpit_core::modules::$mod::clear_all_pids()
            },
            get_default_user_data_dir: || {
                cockpit_core::modules::$mod::$user_data_dir_fn()
            },
            get_instance_defaults: || {
                let defaults = cockpit_core::modules::$mod::get_instance_defaults()?;
                serde_json::to_value(defaults).map_err(|e| format!("serialize failed: {e}"))
            },
            load_instance_store: || {
                cockpit_core::modules::$mod::load_instance_store()
            },
            delete_instance: |id| {
                cockpit_core::modules::$mod::delete_instance(id)
            },
            create_instance_from_json: |params| {
                let p = crate::platform_def::parse_create_instance_params(params)?;
                cockpit_core::modules::$mod::create_instance(p)
            },
            update_instance_from_json: |params| {
                let p = crate::platform_def::parse_update_instance_params(params)?;
                cockpit_core::modules::$mod::update_instance(p)
            },
            supports_instance_launch: true,
        }
    };
}

// codex 使用不同的 CreateInstanceParams/UpdateInstanceParams 类型
macro_rules! impl_codex_platform_def {
    ($slug:expr, $mod:ident, $user_data_dir_fn:ident) => {
        PlatformDef {
            slug: $slug,
            load_default_settings: || {
                cockpit_core::modules::$mod::load_default_settings()
            },
            update_default_pid: |pid| {
                cockpit_core::modules::$mod::update_default_pid(pid)
            },
            update_instance_pid: |id, pid| {
                cockpit_core::modules::$mod::update_instance_pid(id, pid)
            },
            clear_all_pids: || {
                cockpit_core::modules::$mod::clear_all_pids()
            },
            get_default_user_data_dir: || {
                cockpit_core::modules::$mod::$user_data_dir_fn()
            },
            get_instance_defaults: || {
                let defaults = cockpit_core::modules::$mod::get_instance_defaults()?;
                serde_json::to_value(defaults).map_err(|e| format!("serialize failed: {e}"))
            },
            load_instance_store: || {
                cockpit_core::modules::$mod::load_instance_store()
            },
            delete_instance: |id| {
                cockpit_core::modules::$mod::delete_instance(id)
            },
            create_instance_from_json: |params| {
                let p = crate::platform_def::parse_codex_create_instance_params(params)?;
                cockpit_core::modules::$mod::create_instance(p)
            },
            update_instance_from_json: |params| {
                let p = crate::platform_def::parse_codex_update_instance_params(params)?;
                cockpit_core::modules::$mod::update_instance(p)
            },
            supports_instance_launch: true,
        }
    };
}

/// 从 JSON params 构造 instance_store::CreateInstanceParams
pub(crate) fn parse_create_instance_params(
    params: &Value,
) -> Result<cockpit_core::modules::instance_store::CreateInstanceParams, String> {
    use crate::params::*;
    Ok(cockpit_core::modules::instance_store::CreateInstanceParams {
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
    })
}

/// 从 JSON params 构造 instance_store::UpdateInstanceParams
pub(crate) fn parse_update_instance_params(
    params: &Value,
) -> Result<cockpit_core::modules::instance_store::UpdateInstanceParams, String> {
    use crate::params::*;
    Ok(cockpit_core::modules::instance_store::UpdateInstanceParams {
        instance_id: param_string(params, &["instanceId", "instance_id"])?,
        name: param_optional_string(params, &["name"])?,
        working_dir: param_optional_string(params, &["workingDir", "working_dir"])?,
        extra_args: param_optional_string(params, &["extraArgs", "extra_args"])?,
        bind_account_id: param_optional_string(params, &["bindAccountId", "bind_account_id"])?
            .map(Some),
    })
}

/// 从 JSON params 构造 codex_instance::CreateInstanceParams
pub(crate) fn parse_codex_create_instance_params(
    params: &Value,
) -> Result<cockpit_core::modules::codex_instance::CreateInstanceParams, String> {
    use crate::params::*;
    Ok(cockpit_core::modules::codex_instance::CreateInstanceParams {
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
        launch_mode: param_optional_string(params, &["launchMode", "launch_mode"])?
            .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
        app_speed: param_optional_string(params, &["appSpeed", "app_speed"])?
            .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
    })
}

/// 从 JSON params 构造 codex_instance::UpdateInstanceParams
pub(crate) fn parse_codex_update_instance_params(
    params: &Value,
) -> Result<cockpit_core::modules::codex_instance::UpdateInstanceParams, String> {
    use crate::params::*;
    Ok(cockpit_core::modules::codex_instance::UpdateInstanceParams {
        instance_id: param_string(params, &["instanceId", "instance_id"])?,
        name: param_optional_string(params, &["name"])?,
        working_dir: param_optional_string(params, &["workingDir", "working_dir"])?,
        extra_args: param_optional_string(params, &["extraArgs", "extra_args"])?,
        bind_account_id: param_optional_string(params, &["bindAccountId", "bind_account_id"])?
            .map(Some),
        launch_mode: param_optional_string(params, &["launchMode", "launch_mode"])?
            .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
        app_speed: param_optional_string(params, &["appSpeed", "app_speed"])?
            .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
    })
}

fn codex_def() -> PlatformDef {
    impl_codex_platform_def!("codex", codex_instance, get_default_codex_home)
}

fn github_copilot_def() -> PlatformDef {
    impl_platform_def!("github_copilot", github_copilot_instance, get_default_vscode_user_data_dir)
}

fn windsurf_def() -> PlatformDef {
    impl_platform_def!("windsurf", windsurf_instance, get_default_windsurf_user_data_dir)
}

fn kiro_def() -> PlatformDef {
    impl_platform_def!("kiro", kiro_instance, get_default_kiro_user_data_dir)
}

fn cursor_def() -> PlatformDef {
    impl_platform_def!("cursor", cursor_instance, get_default_cursor_user_data_dir)
}

fn gemini_def() -> PlatformDef {
    impl_platform_def!("gemini", gemini_instance, get_default_gemini_cli_home_root)
}

fn codebuddy_def() -> PlatformDef {
    impl_platform_def!("codebuddy", codebuddy_instance, get_default_codebuddy_user_data_dir)
}

fn codebuddy_cn_def() -> PlatformDef {
    impl_platform_def!("codebuddy_cn", codebuddy_cn_instance, get_default_codebuddy_cn_user_data_dir)
}

fn qoder_def() -> PlatformDef {
    impl_platform_def!("qoder", qoder_instance, get_default_qoder_user_data_dir)
}

fn trae_def() -> PlatformDef {
    impl_platform_def!("trae", trae_instance, get_default_trae_user_data_dir)
}

fn workbuddy_def() -> PlatformDef {
    impl_platform_def!("workbuddy", workbuddy_instance, get_default_workbuddy_user_data_dir)
}

/// 所有 provider 的注册表
/// 新增 provider 只需在此处添加一行
pub(crate) static PLATFORMS: &[(&str, fn() -> PlatformDef)] = &[
    ("antigravity", antigravity_def),
    ("codex", codex_def),
    ("github_copilot", github_copilot_def),
    ("windsurf", windsurf_def),
    ("kiro", kiro_def),
    ("cursor", cursor_def),
    ("gemini", gemini_def),
    ("codebuddy", codebuddy_def),
    ("codebuddy_cn", codebuddy_cn_def),
    ("qoder", qoder_def),
    ("trae", trae_def),
    ("workbuddy", workbuddy_def),
];

/// 查找平台定义
pub(crate) fn find_platform(slug: &str) -> Result<&'static PlatformDef, String> {
    // 使用 LazyLock 预创建所有平台定义
    static PLATFORM_MAP: std::sync::LazyLock<Vec<PlatformDef>> = std::sync::LazyLock::new(|| {
        PLATFORMS.iter().map(|(_, factory)| factory()).collect()
    });
    PLATFORM_MAP
        .iter()
        .find(|p| p.slug == slug)
        .ok_or_else(|| format!("unknown platform: {slug}"))
}

/// 获取所有平台 slug 列表
pub(crate) fn all_platform_slugs() -> Vec<&'static str> {
    PLATFORMS.iter().map(|(s, _)| *s).collect()
}
