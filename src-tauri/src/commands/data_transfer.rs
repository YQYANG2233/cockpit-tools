use tauri_plugin_autostart::ManagerExt as _;

use cockpit_core::models::InstanceStore;
use cockpit_core::modules::data_transfer as core_data_transfer;

use crate::modules;
use crate::modules::config::{self, UserConfig};
use crate::modules::websocket;

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

#[tauri::command]
pub fn data_transfer_get_user_config() -> Result<UserConfig, String> {
    Ok(config::get_user_config())
}

#[tauri::command]
pub fn data_transfer_apply_user_config(
    app: tauri::AppHandle,
    config: UserConfig,
) -> Result<bool, String> {
    let current = config::get_user_config();
    let imported_config = config;

    // 恢复备份配置时，保留当前的 WebDAV 同步配置与同步历史状态，避免被覆盖或重置
    let current_app_auto_launch_enabled =
        get_app_auto_launch_enabled(&app).unwrap_or(current.app_auto_launch_enabled);

    let applied = core_data_transfer::apply_user_config(
        current,
        imported_config,
        current_app_auto_launch_enabled,
    )?;

    if applied.app_auto_launch_changed {
        apply_app_auto_launch_enabled(&app, applied.config.app_auto_launch_enabled)?;
    }

    if let Err(err) = modules::floating_card_window::apply_floating_card_always_on_top(&app) {
        modules::logger::log_warn(&format!("[DataTransfer] 应用悬浮卡片置顶状态失败: {}", err));
    }

    #[cfg(target_os = "macos")]
    if applied.hide_dock_icon_changed {
        crate::apply_macos_activation_policy(&app);
    }

    #[cfg(target_os = "macos")]
    if applied.tray_icon_style_changed {
        if let Err(err) = modules::tray::apply_tray_icon_style(&app) {
            modules::logger::log_warn(&format!(
                "[DataTransfer] 应用 macOS 菜单栏图标样式失败: {}",
                err
            ));
        }
    }

    if applied.language_changed {
        let normalized_language = applied.config.language.clone();
        websocket::broadcast_language_changed(&normalized_language, "desktop");
        modules::sync_settings::write_sync_setting("language", &normalized_language);
        if let Err(err) = modules::tray::update_tray_menu(&app) {
            modules::logger::log_warn(&format!("[DataTransfer] 语言变更后刷新托盘失败: {}", err));
        }
    }

    Ok(applied.needs_restart)
}

#[tauri::command]
pub fn data_transfer_get_instance_store(platform: String) -> Result<InstanceStore, String> {
    core_data_transfer::load_instance_store_by_platform(platform.trim())
}

#[tauri::command]
pub fn data_transfer_replace_instance_store(
    platform: String,
    store: InstanceStore,
) -> Result<(), String> {
    core_data_transfer::replace_instance_store(platform.trim(), &store)
}
