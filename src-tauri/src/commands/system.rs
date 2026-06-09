use std::path::PathBuf;
use std::time::{Duration, Instant};

use tauri::Manager;
use tauri_plugin_autostart::ManagerExt as _;

use crate::modules;
use crate::modules::config::{self, NetworkConfig, NetworkConfigUpdate};
use crate::modules::web_report;
use crate::modules::websocket;

pub type GeneralConfig = cockpit_core::modules::config::GeneralConfig;

pub use cockpit_core::modules::antigravity_runtime::{
    get_cached_antigravity_installed_version_info_for_target,
    resolve_antigravity_installed_version_info_for_target, AntigravityInstalledVersionInfo,
};

pub use cockpit_core::modules::auto_backup::{
    AutoBackupFileEntry, AutoBackupSettings, WebdavSyncSettings,
};

fn get_app_auto_launch_enabled(app: &tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|err| format!("读取应用自启动状态失败: {}", err))
}

fn apply_app_auto_launch_enabled(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch()
            .enable()
            .map_err(|err| format!("启用应用自启动失败: {}", err))
    } else {
        app.autolaunch()
            .disable()
            .map_err(|err| format!("停用应用自启动失败: {}", err))
    }
}

fn resolve_downloads_dir() -> Result<PathBuf, String> {
    cockpit_core::modules::system_host::downloads_dir()
}

#[tauri::command]
pub async fn open_data_folder() -> Result<(), String> {
    cockpit_core::modules::system_host::open_data_folder()
}

/// 保存文本文件
#[tauri::command]
pub async fn save_text_file(path: String, content: String) -> Result<(), String> {
    cockpit_core::modules::system_host::save_text_file(path, &content)
}

/// 获取下载目录
#[tauri::command]
pub fn get_downloads_dir() -> Result<String, String> {
    Ok(resolve_downloads_dir()?.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_auto_backup_settings() -> Result<AutoBackupSettings, String> {
    cockpit_core::modules::auto_backup::get_auto_backup_settings()
}

#[tauri::command]
pub fn save_auto_backup_settings(
    enabled: bool,
    include_accounts: bool,
    include_config: bool,
    retention_days: i32,
) -> Result<AutoBackupSettings, String> {
    cockpit_core::modules::auto_backup::save_auto_backup_settings(
        enabled,
        include_accounts,
        include_config,
        retention_days,
    )
}

#[tauri::command]
pub fn update_auto_backup_last_run(
    last_backup_at: Option<String>,
) -> Result<AutoBackupSettings, String> {
    cockpit_core::modules::auto_backup::update_auto_backup_last_run(last_backup_at)
}

#[tauri::command]
pub fn write_auto_backup_file(file_name: String, content: String) -> Result<String, String> {
    cockpit_core::modules::auto_backup::write_auto_backup_file(&file_name, &content)
}

#[tauri::command]
pub fn read_auto_backup_file(file_name: String) -> Result<String, String> {
    cockpit_core::modules::auto_backup::read_auto_backup_file(&file_name)
}

#[tauri::command]
pub fn copy_auto_backup_file(file_name: String, target_path: String) -> Result<String, String> {
    cockpit_core::modules::auto_backup::copy_auto_backup_file(&file_name, &target_path)
}

#[tauri::command]
pub fn list_auto_backup_files() -> Result<Vec<AutoBackupFileEntry>, String> {
    cockpit_core::modules::auto_backup::list_auto_backup_files()
}

#[tauri::command]
pub fn delete_auto_backup_file(file_name: String) -> Result<(), String> {
    cockpit_core::modules::auto_backup::delete_auto_backup_file(&file_name)
}

#[tauri::command]
pub fn cleanup_auto_backup_files(retention_days: i32) -> Result<Vec<String>, String> {
    cockpit_core::modules::auto_backup::cleanup_auto_backup_files(retention_days)
}

#[tauri::command]
pub fn open_auto_backup_dir() -> Result<(), String> {
    cockpit_core::modules::auto_backup::open_auto_backup_dir()
}

#[tauri::command]
pub fn get_webdav_sync_settings() -> Result<WebdavSyncSettings, String> {
    Ok(cockpit_core::modules::auto_backup::get_webdav_sync_settings())
}

#[tauri::command]
pub fn save_webdav_sync_settings(
    enabled: bool,
    url: String,
    username: String,
    password: Option<String>,
    clear_password: Option<bool>,
    remote_dir: String,
    retention_days: i32,
) -> Result<WebdavSyncSettings, String> {
    cockpit_core::modules::auto_backup::save_webdav_sync_settings(
        enabled,
        &url,
        &username,
        password,
        clear_password,
        &remote_dir,
        Some(retention_days),
    )
}

#[tauri::command]
pub async fn test_webdav_sync_connection(
    url: String,
    username: String,
    password: Option<String>,
    clear_password: Option<bool>,
    remote_dir: String,
) -> Result<modules::webdav_sync::WebdavTestResult, String> {
    cockpit_core::modules::auto_backup::test_webdav_sync_connection(
        &url,
        &username,
        password,
        clear_password,
        &remote_dir,
    )
    .await
}

#[tauri::command]
pub async fn upload_auto_backup_to_webdav(
    file_name: String,
) -> Result<modules::webdav_sync::WebdavUploadResult, String> {
    cockpit_core::modules::auto_backup::upload_auto_backup_to_webdav(&file_name).await
}

#[tauri::command]
pub async fn list_webdav_backup_files(
) -> Result<Vec<modules::webdav_sync::WebdavBackupFileEntry>, String> {
    cockpit_core::modules::auto_backup::list_webdav_backup_files().await
}

#[tauri::command]
pub async fn read_webdav_backup_file(file_name: String) -> Result<String, String> {
    cockpit_core::modules::auto_backup::read_webdav_backup_file(&file_name).await
}

#[tauri::command]
pub async fn delete_webdav_backup_file(file_name: String) -> Result<(), String> {
    cockpit_core::modules::auto_backup::delete_webdav_backup_file(&file_name).await
}
/// 获取网络服务配置
#[tauri::command]
pub fn get_network_config() -> Result<NetworkConfig, String> {
    Ok(config::get_network_config(web_report::get_actual_port()))
}

/// 保存网络服务配置
#[tauri::command]
pub fn save_network_config(
    ws_enabled: bool,
    ws_port: u16,
    report_enabled: Option<bool>,
    report_port: Option<u16>,
    report_token: Option<String>,
    global_proxy_enabled: Option<bool>,
    global_proxy_url: Option<String>,
    global_proxy_no_proxy: Option<String>,
) -> Result<bool, String> {
    config::save_network_config(NetworkConfigUpdate {
        ws_enabled,
        ws_port,
        report_enabled,
        report_port,
        report_token,
        global_proxy_enabled,
        global_proxy_url,
        global_proxy_no_proxy,
    })
}

/// 获取系统可用的终端列表
#[tauri::command]
pub async fn get_available_terminals() -> Result<Vec<String>, String> {
    Ok(cockpit_core::modules::system_host::available_terminals())
}

/// 获取通用设置配置
#[tauri::command]
pub fn get_general_config(app: tauri::AppHandle) -> Result<GeneralConfig, String> {
    let started = Instant::now();
    let mut user_config = config::get_user_config();
    let app_auto_launch_enabled =
        get_app_auto_launch_enabled(&app).unwrap_or(user_config.app_auto_launch_enabled);
    if app_auto_launch_enabled != user_config.app_auto_launch_enabled {
        user_config.app_auto_launch_enabled = app_auto_launch_enabled;
        if let Err(err) = config::save_user_config(&user_config) {
            modules::logger::log_warn(&format!(
                "[SystemConfig] 同步应用自启动状态到本地配置失败: {}",
                err
            ));
        }
    }

    let result =
        config::general_config_from_user_config(user_config, Some(app_auto_launch_enabled));

    modules::logger::log_info(&format!(
        "[StartupPerf][SystemCommand] get_general_config completed in {}ms: auto_refresh={}, codex={}, zed={}, ghcp={}, windsurf={}, kiro={}, cursor={}, gemini={}, codebuddy={}, codebuddy_cn={}, workbuddy={}, qoder={}, trae={}, auto_switch={}",
        started.elapsed().as_millis(),
        result.auto_refresh_minutes,
        result.codex_auto_refresh_minutes,
        result.zed_auto_refresh_minutes,
        result.ghcp_auto_refresh_minutes,
        result.windsurf_auto_refresh_minutes,
        result.kiro_auto_refresh_minutes,
        result.cursor_auto_refresh_minutes,
        result.gemini_auto_refresh_minutes,
        result.codebuddy_auto_refresh_minutes,
        result.codebuddy_cn_auto_refresh_minutes,
        result.workbuddy_auto_refresh_minutes,
        result.qoder_auto_refresh_minutes,
        result.trae_auto_refresh_minutes,
        result.auto_switch_enabled
    ));

    Ok(result)
}

/// 保存通用设置配置
#[tauri::command]
pub fn save_general_config(
    app: tauri::AppHandle,
    language: String,
    default_terminal: Option<String>,
    theme: String,
    ui_scale: Option<f64>,
    auto_refresh_minutes: i32,
    codex_auto_refresh_minutes: i32,
    codex_sync_wsl: Option<bool>,
    codex_wsl_config_dir: Option<String>,
    zed_auto_refresh_minutes: Option<i32>,
    ghcp_auto_refresh_minutes: Option<i32>,
    windsurf_auto_refresh_minutes: Option<i32>,
    kiro_auto_refresh_minutes: Option<i32>,
    cursor_auto_refresh_minutes: Option<i32>,
    gemini_auto_refresh_minutes: Option<i32>,
    gemini_sync_wsl: Option<bool>,
    codebuddy_auto_refresh_minutes: Option<i32>,
    codebuddy_cn_auto_refresh_minutes: Option<i32>,
    workbuddy_auto_refresh_minutes: Option<i32>,
    qoder_auto_refresh_minutes: Option<i32>,
    trae_auto_refresh_minutes: Option<i32>,
    close_behavior: String,
    minimize_behavior: Option<String>,
    hide_dock_icon: Option<bool>,
    tray_icon_style: Option<String>,
    floating_card_show_on_startup: Option<bool>,
    floating_card_always_on_top: Option<bool>,
    app_auto_launch_enabled: Option<bool>,
    antigravity_startup_wakeup_enabled: Option<bool>,
    antigravity_startup_wakeup_delay_seconds: Option<i32>,
    codex_startup_wakeup_enabled: Option<bool>,
    codex_startup_wakeup_delay_seconds: Option<i32>,
    floating_card_confirm_on_close: Option<bool>,
    opencode_app_path: String,
    antigravity_app_path: String,
    codex_app_path: String,
    codex_specified_app_path: Option<String>,
    zed_app_path: Option<String>,
    vscode_app_path: String,
    windsurf_app_path: Option<String>,
    kiro_app_path: Option<String>,
    cursor_app_path: Option<String>,
    codebuddy_app_path: Option<String>,
    codebuddy_cn_app_path: Option<String>,
    qoder_app_path: Option<String>,
    trae_app_path: Option<String>,
    workbuddy_app_path: Option<String>,
    opencode_sync_on_switch: bool,
    opencode_auth_overwrite_on_switch: Option<bool>,
    ghcp_opencode_sync_on_switch: Option<bool>,
    ghcp_opencode_auth_overwrite_on_switch: Option<bool>,
    ghcp_launch_on_switch: Option<bool>,
    openclaw_auth_overwrite_on_switch: Option<bool>,
    codex_launch_on_switch: bool,
    codex_restart_specified_app_on_switch: Option<bool>,
    codex_local_access_entry_visible: Option<bool>,
    top_right_ad_visible: Option<bool>,
    antigravity_dual_switch_no_restart_enabled: Option<bool>,
    auto_switch_enabled: Option<bool>,
    auto_switch_threshold: Option<i32>,
    auto_switch_credits_enabled: Option<bool>,
    auto_switch_credits_threshold: Option<i32>,
    auto_switch_scope_mode: Option<String>,
    auto_switch_selected_group_ids: Option<Vec<String>>,
    auto_switch_account_scope_mode: Option<String>,
    auto_switch_selected_account_ids: Option<Vec<String>>,
    codex_auto_switch_enabled: Option<bool>,
    codex_auto_switch_primary_threshold: Option<i32>,
    codex_auto_switch_secondary_threshold: Option<i32>,
    codex_auto_switch_account_scope_mode: Option<String>,
    codex_auto_switch_selected_account_ids: Option<Vec<String>>,
    quota_alert_enabled: Option<bool>,
    quota_alert_threshold: Option<i32>,
    codex_quota_alert_enabled: Option<bool>,
    codex_quota_alert_threshold: Option<i32>,
    zed_quota_alert_enabled: Option<bool>,
    zed_quota_alert_threshold: Option<i32>,
    codex_quota_alert_primary_threshold: Option<i32>,
    codex_quota_alert_secondary_threshold: Option<i32>,
    ghcp_quota_alert_enabled: Option<bool>,
    ghcp_quota_alert_threshold: Option<i32>,
    windsurf_quota_alert_enabled: Option<bool>,
    windsurf_quota_alert_threshold: Option<i32>,
    kiro_quota_alert_enabled: Option<bool>,
    kiro_quota_alert_threshold: Option<i32>,
    cursor_quota_alert_enabled: Option<bool>,
    cursor_quota_alert_threshold: Option<i32>,
    gemini_quota_alert_enabled: Option<bool>,
    gemini_quota_alert_threshold: Option<i32>,
    codebuddy_quota_alert_enabled: Option<bool>,
    codebuddy_quota_alert_threshold: Option<i32>,
    codebuddy_cn_quota_alert_enabled: Option<bool>,
    codebuddy_cn_quota_alert_threshold: Option<i32>,
    qoder_quota_alert_enabled: Option<bool>,
    qoder_quota_alert_threshold: Option<i32>,
    trae_quota_alert_enabled: Option<bool>,
    trae_quota_alert_threshold: Option<i32>,
    workbuddy_quota_alert_enabled: Option<bool>,
    workbuddy_quota_alert_threshold: Option<i32>,
) -> Result<(), String> {
    let current = config::get_user_config();
    let new_config = config::resolve_general_config_save(
        current.clone(),
        config::GeneralConfigSaveInput {
            language,
            default_terminal,
            theme,
            ui_scale,
            auto_refresh_minutes,
            codex_auto_refresh_minutes,
            codex_sync_wsl,
            codex_wsl_config_dir,
            zed_auto_refresh_minutes,
            ghcp_auto_refresh_minutes,
            windsurf_auto_refresh_minutes,
            kiro_auto_refresh_minutes,
            cursor_auto_refresh_minutes,
            gemini_auto_refresh_minutes,
            gemini_sync_wsl,
            codebuddy_auto_refresh_minutes,
            codebuddy_cn_auto_refresh_minutes,
            workbuddy_auto_refresh_minutes,
            qoder_auto_refresh_minutes,
            trae_auto_refresh_minutes,
            close_behavior,
            minimize_behavior,
            hide_dock_icon,
            tray_icon_style,
            floating_card_show_on_startup,
            floating_card_always_on_top,
            app_auto_launch_enabled,
            antigravity_startup_wakeup_enabled,
            antigravity_startup_wakeup_delay_seconds,
            codex_startup_wakeup_enabled,
            codex_startup_wakeup_delay_seconds,
            floating_card_confirm_on_close,
            opencode_app_path,
            antigravity_app_path,
            codex_app_path,
            codex_specified_app_path,
            zed_app_path,
            vscode_app_path,
            windsurf_app_path,
            kiro_app_path,
            cursor_app_path,
            codebuddy_app_path,
            codebuddy_cn_app_path,
            qoder_app_path,
            trae_app_path,
            workbuddy_app_path,
            opencode_sync_on_switch,
            opencode_auth_overwrite_on_switch,
            ghcp_opencode_sync_on_switch,
            ghcp_opencode_auth_overwrite_on_switch,
            ghcp_launch_on_switch,
            openclaw_auth_overwrite_on_switch,
            codex_launch_on_switch,
            codex_restart_specified_app_on_switch,
            codex_local_access_entry_visible,
            top_right_ad_visible,
            antigravity_dual_switch_no_restart_enabled,
            auto_switch_enabled,
            auto_switch_threshold,
            auto_switch_credits_enabled,
            auto_switch_credits_threshold,
            auto_switch_scope_mode,
            auto_switch_selected_group_ids,
            auto_switch_account_scope_mode,
            auto_switch_selected_account_ids,
            codex_auto_switch_enabled,
            codex_auto_switch_primary_threshold,
            codex_auto_switch_secondary_threshold,
            codex_auto_switch_account_scope_mode,
            codex_auto_switch_selected_account_ids,
            quota_alert_enabled,
            quota_alert_threshold,
            codex_quota_alert_enabled,
            codex_quota_alert_threshold,
            zed_quota_alert_enabled,
            zed_quota_alert_threshold,
            codex_quota_alert_primary_threshold,
            codex_quota_alert_secondary_threshold,
            ghcp_quota_alert_enabled,
            ghcp_quota_alert_threshold,
            windsurf_quota_alert_enabled,
            windsurf_quota_alert_threshold,
            kiro_quota_alert_enabled,
            kiro_quota_alert_threshold,
            cursor_quota_alert_enabled,
            cursor_quota_alert_threshold,
            gemini_quota_alert_enabled,
            gemini_quota_alert_threshold,
            codebuddy_quota_alert_enabled,
            codebuddy_quota_alert_threshold,
            codebuddy_cn_quota_alert_enabled,
            codebuddy_cn_quota_alert_threshold,
            qoder_quota_alert_enabled,
            qoder_quota_alert_threshold,
            trae_quota_alert_enabled,
            trae_quota_alert_threshold,
            workbuddy_quota_alert_enabled,
            workbuddy_quota_alert_threshold,
        },
    );

    let language_changed = current.language != new_config.language;
    let language_for_broadcast = new_config.language.clone();
    let current_app_auto_launch_enabled = current.app_auto_launch_enabled;
    let app_auto_launch_enabled_value = new_config.app_auto_launch_enabled;
    #[cfg(target_os = "macos")]
    let hide_dock_icon_changed = current.hide_dock_icon != new_config.hide_dock_icon;
    #[cfg(target_os = "macos")]
    let tray_icon_style_changed = current.tray_icon_style != new_config.tray_icon_style;

    config::save_user_config(&new_config)?;

    if current_app_auto_launch_enabled != app_auto_launch_enabled_value {
        apply_app_auto_launch_enabled(&app, app_auto_launch_enabled_value)?;
    }

    if let Err(err) = modules::floating_card_window::apply_floating_card_always_on_top(&app) {
        modules::logger::log_warn(&format!(
            "[FloatingCard] 保存通用设置后应用置顶状态失败: {}",
            err
        ));
    }

    #[cfg(target_os = "macos")]
    if hide_dock_icon_changed {
        crate::apply_macos_activation_policy(&app);
    }

    #[cfg(target_os = "macos")]
    if tray_icon_style_changed {
        if let Err(err) = modules::tray::apply_tray_icon_style(&app) {
            modules::logger::log_warn(&format!("[Tray] 保存通用设置后应用图标样式失败: {}", err));
        }
    }

    if language_changed {
        // 广播语言变更（如果有客户端连接，会通过 WebSocket 发送）
        websocket::broadcast_language_changed(&language_for_broadcast, "desktop");

        // 同时写入共享文件（供插件端离线时启动读取）
        // 因为无法确定插件端是否收到了 WebSocket 消息，保守策略是总是写入
        // 但为了减少写入，可以检查是否有客户端连接
        // 这里简化处理：总是写入，插件端启动时会比较时间戳
        modules::sync_settings::write_sync_setting("language", &language_for_broadcast);

        // 仅在语言变更时刷新托盘菜单，避免无关配置触发托盘重建
        if let Err(err) = modules::tray::update_tray_menu(&app) {
            modules::logger::log_warn(&format!("[Tray] 语言变更后刷新托盘失败: {}", err));
        }
    }

    Ok(())
}

#[tauri::command]
pub fn save_tray_platform_layout(
    app: tauri::AppHandle,
    sort_mode: String,
    ordered_platform_ids: Vec<String>,
    tray_platform_ids: Vec<String>,
    ordered_entry_ids: Option<Vec<String>>,
    platform_groups: Option<Vec<modules::tray_layout::TrayLayoutGroup>>,
) -> Result<(), String> {
    modules::tray_layout::save_tray_layout(
        sort_mode,
        ordered_platform_ids,
        tray_platform_ids,
        ordered_entry_ids,
        platform_groups,
    )?;
    modules::tray::update_tray_menu(&app)?;
    Ok(())
}

#[tauri::command]
pub fn set_app_path(app: String, path: String) -> Result<(), String> {
    config::set_app_path(&app, path)
}

#[tauri::command]
pub fn set_codex_launch_on_switch(enabled: bool) -> Result<(), String> {
    config::set_codex_launch_on_switch(enabled)
}

#[tauri::command]
pub fn set_codex_local_access_entry_visible(enabled: bool) -> Result<(), String> {
    config::set_codex_local_access_entry_visible(enabled)
}

#[tauri::command]
pub fn detect_app_path(app: String, force: Option<bool>) -> Result<Option<String>, String> {
    cockpit_core::modules::system_host::detect_app_path(&app, force.unwrap_or(false))
}

#[tauri::command]
pub async fn get_antigravity_installed_version_info(
    target: Option<String>,
    scan_mode: Option<String>,
) -> Result<Option<AntigravityInstalledVersionInfo>, String> {
    let scan_mode =
        cockpit_core::modules::antigravity_runtime::normalize_antigravity_version_scan_mode(
            scan_mode.as_deref(),
        );
    let timeout_ms =
        cockpit_core::modules::antigravity_runtime::antigravity_version_timeout_ms(scan_mode);
    let target_for_task = target.clone();

    let task = tauri::async_runtime::spawn_blocking(move || {
        cockpit_core::modules::antigravity_runtime::resolve_antigravity_installed_version_info_for_target_with_scan_mode(
            target_for_task.as_deref(),
            scan_mode,
        )
    });

    match tokio::time::timeout(Duration::from_millis(timeout_ms), task).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => Err(format!("Antigravity 版本检测任务失败: {}", error)),
        Err(_) => Ok(None),
    }
}

/// 通知插件关闭/开启唤醒功能（互斥）
#[tauri::command]
pub fn set_wakeup_override(enabled: bool) -> Result<(), String> {
    websocket::broadcast_wakeup_override(enabled);
    Ok(())
}

/// 执行窗口关闭操作
/// action: "minimize" | "quit"
/// remember: 是否记住选择
#[tauri::command]
pub fn handle_window_close(
    window: tauri::Window,
    action: String,
    remember: bool,
) -> Result<(), String> {
    modules::logger::log_info(&format!(
        "[Window] 用户选择: action={}, remember={}",
        action, remember
    ));

    // 如果需要记住选择，更新配置
    if remember {
        config::save_close_behavior_for_window_action(&action)?;
        modules::logger::log_info(&format!("[Window] 已保存关闭行为设置: {}", action));
    }

    // 执行操作
    match action.as_str() {
        "minimize" => {
            let _ = window.hide();
            modules::logger::log_info("[Window] 窗口已最小化到托盘");
        }
        "quit" => {
            window.app_handle().exit(0);
        }
        _ => {
            return Err("无效的操作".to_string());
        }
    }

    Ok(())
}

#[tauri::command]
pub fn show_floating_card_window(app: tauri::AppHandle) -> Result<(), String> {
    modules::floating_card_window::show_floating_card_window(&app, true)
}

#[tauri::command]
pub fn show_instance_floating_card_window(
    app: tauri::AppHandle,
    context: modules::floating_card_window::FloatingCardInstanceContext,
) -> Result<(), String> {
    modules::floating_card_window::show_instance_floating_card_window(&app, context, true)
}

#[tauri::command]
pub fn get_floating_card_context(
    window_label: String,
) -> Result<Option<modules::floating_card_window::FloatingCardInstanceContext>, String> {
    modules::floating_card_window::get_floating_card_context(&window_label)
}

#[tauri::command]
pub fn hide_floating_card_window(app: tauri::AppHandle) -> Result<(), String> {
    modules::floating_card_window::hide_floating_card_window(&app, false)
}

#[tauri::command]
pub fn hide_current_floating_card_window(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn set_floating_card_always_on_top(
    app: tauri::AppHandle,
    always_on_top: bool,
) -> Result<(), String> {
    config::set_floating_card_always_on_top(always_on_top)?;
    modules::floating_card_window::apply_floating_card_always_on_top(&app)
}

#[tauri::command]
pub fn set_current_floating_card_window_always_on_top(
    window: tauri::Window,
    always_on_top: bool,
) -> Result<(), String> {
    window
        .set_always_on_top(always_on_top)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn set_floating_card_confirm_on_close(confirm_on_close: bool) -> Result<(), String> {
    config::set_floating_card_confirm_on_close(confirm_on_close)
}

#[tauri::command]
pub fn save_floating_card_position(x: i32, y: i32) -> Result<(), String> {
    config::save_floating_card_position(x, y)
}

#[tauri::command]
pub fn show_main_window_and_navigate(app: tauri::AppHandle, page: String) -> Result<(), String> {
    modules::floating_card_window::show_main_window_and_navigate(&app, &page)
}

#[tauri::command]
pub fn external_import_take_pending(
) -> Option<modules::external_import::ExternalProviderImportPayload> {
    modules::external_import::take_pending_external_import()
}

#[tauri::command]
pub async fn external_import_fetch_import_url(import_url: String) -> Result<String, String> {
    cockpit_core::modules::external_import::fetch_external_import_url(&import_url).await
}

/// 打开指定文件夹（如不存在则创建）
#[tauri::command]
pub async fn open_folder(path: String) -> Result<(), String> {
    cockpit_core::modules::system_host::open_folder(path)
}

/// 删除损坏的文件（会先备份）
#[tauri::command]
pub async fn delete_corrupted_file(path: String) -> Result<(), String> {
    cockpit_core::modules::corrupted_file::backup_corrupted_file(path).map(|_| ())
}
