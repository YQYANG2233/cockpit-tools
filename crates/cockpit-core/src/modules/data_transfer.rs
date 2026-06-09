use crate::models::InstanceStore;
use crate::modules::{
    codebuddy_cn_instance, codebuddy_instance, codex_instance, config, cursor_instance,
    gemini_instance, github_copilot_instance, instance, kiro_instance, qoder_instance,
    trae_instance, windsurf_instance, workbuddy_instance,
};

#[derive(Debug, Clone)]
pub struct AppliedUserConfig {
    pub config: config::UserConfig,
    pub needs_restart: bool,
    pub language_changed: bool,
    pub app_auto_launch_changed: bool,
    pub hide_dock_icon_changed: bool,
    pub tray_icon_style_changed: bool,
}

pub fn resolve_apply_user_config(
    current: config::UserConfig,
    mut imported: config::UserConfig,
    current_app_auto_launch_enabled: bool,
) -> AppliedUserConfig {
    imported.webdav_sync_enabled = current.webdav_sync_enabled;
    imported.webdav_sync_url = current.webdav_sync_url.clone();
    imported.webdav_sync_username = current.webdav_sync_username.clone();
    imported.webdav_sync_password = current.webdav_sync_password.clone();
    imported.webdav_sync_remote_dir = current.webdav_sync_remote_dir.clone();
    imported.webdav_sync_retention_days = current.webdav_sync_retention_days;
    imported.webdav_sync_last_upload_at = current.webdav_sync_last_upload_at.clone();
    imported.webdav_sync_last_upload_file_name = current.webdav_sync_last_upload_file_name.clone();
    imported.webdav_sync_last_download_at = current.webdav_sync_last_download_at.clone();
    imported.webdav_sync_last_download_file_name =
        current.webdav_sync_last_download_file_name.clone();

    AppliedUserConfig {
        needs_restart: current.ws_port != imported.ws_port
            || current.ws_enabled != imported.ws_enabled
            || current.report_enabled != imported.report_enabled
            || current.report_port != imported.report_port
            || current.report_token != imported.report_token,
        language_changed: current.language != imported.language,
        app_auto_launch_changed: current_app_auto_launch_enabled
            != imported.app_auto_launch_enabled,
        hide_dock_icon_changed: current.hide_dock_icon != imported.hide_dock_icon,
        tray_icon_style_changed: current.tray_icon_style != imported.tray_icon_style,
        config: imported,
    }
}

pub fn apply_user_config(
    current: config::UserConfig,
    imported: config::UserConfig,
    current_app_auto_launch_enabled: bool,
) -> Result<AppliedUserConfig, String> {
    let result = resolve_apply_user_config(current, imported, current_app_auto_launch_enabled);
    config::save_user_config(&result.config)?;
    Ok(result)
}

pub fn load_instance_store_by_platform(platform: &str) -> Result<InstanceStore, String> {
    match platform.trim() {
        "antigravity" => instance::load_instance_store(),
        "codex" => codex_instance::load_instance_store(),
        "github-copilot" => github_copilot_instance::load_instance_store(),
        "windsurf" => windsurf_instance::load_instance_store(),
        "kiro" => kiro_instance::load_instance_store(),
        "cursor" => cursor_instance::load_instance_store(),
        "gemini" => gemini_instance::load_instance_store(),
        "codebuddy" => codebuddy_instance::load_instance_store(),
        "codebuddy_cn" => codebuddy_cn_instance::load_instance_store(),
        "qoder" => qoder_instance::load_instance_store(),
        "trae" => trae_instance::load_instance_store(),
        "workbuddy" => workbuddy_instance::load_instance_store(),
        _ => Err("不支持的实例平台".to_string()),
    }
}

pub fn save_instance_store_by_platform(
    platform: &str,
    store: &InstanceStore,
) -> Result<(), String> {
    match platform.trim() {
        "antigravity" => instance::save_instance_store(store),
        "codex" => codex_instance::save_instance_store(store),
        "github-copilot" => github_copilot_instance::save_instance_store(store),
        "windsurf" => windsurf_instance::save_instance_store(store),
        "kiro" => kiro_instance::save_instance_store(store),
        "cursor" => cursor_instance::save_instance_store(store),
        "gemini" => gemini_instance::save_instance_store(store),
        "codebuddy" => codebuddy_instance::save_instance_store(store),
        "codebuddy_cn" => codebuddy_cn_instance::save_instance_store(store),
        "qoder" => qoder_instance::save_instance_store(store),
        "trae" => trae_instance::save_instance_store(store),
        "workbuddy" => workbuddy_instance::save_instance_store(store),
        _ => Err("不支持的实例平台".to_string()),
    }
}

pub fn sanitize_instance_store(store: &InstanceStore) -> InstanceStore {
    let mut next = store.clone();
    next.default_settings.last_pid = None;
    for instance in &mut next.instances {
        instance.last_pid = None;
        instance.last_launched_at = None;
    }
    next
}

pub fn replace_instance_store(platform: &str, store: &InstanceStore) -> Result<(), String> {
    let sanitized = sanitize_instance_store(store);
    save_instance_store_by_platform(platform, &sanitized)
}

#[cfg(test)]
mod tests {
    use super::{resolve_apply_user_config, sanitize_instance_store};
    use crate::models::{InstanceLaunchMode, InstanceProfile, InstanceStore};
    use crate::modules::config::UserConfig;

    #[test]
    fn apply_user_config_preserves_webdav_settings_and_reports_changes() {
        let mut current = UserConfig::default();
        current.language = "zh-cn".to_string();
        current.ws_enabled = true;
        current.ws_port = 19528;
        current.report_enabled = false;
        current.report_port = 19529;
        current.report_token = "current-token".to_string();
        current.app_auto_launch_enabled = false;
        current.webdav_sync_enabled = true;
        current.webdav_sync_url = "https://webdav.example.com".to_string();
        current.webdav_sync_username = "current-user".to_string();
        current.webdav_sync_password = "current-password".to_string();
        current.webdav_sync_remote_dir = "/current".to_string();
        current.webdav_sync_retention_days = 9;
        current.webdav_sync_last_upload_at = Some("upload-at".to_string());
        current.webdav_sync_last_upload_file_name = Some("upload.json".to_string());
        current.webdav_sync_last_download_at = Some("download-at".to_string());
        current.webdav_sync_last_download_file_name = Some("download.json".to_string());

        let mut imported = current.clone();
        imported.language = "en".to_string();
        imported.ws_port = 19599;
        imported.app_auto_launch_enabled = true;
        imported.webdav_sync_enabled = false;
        imported.webdav_sync_url = "https://imported.example.com".to_string();
        imported.webdav_sync_username = "imported-user".to_string();
        imported.webdav_sync_password = "imported-password".to_string();
        imported.webdav_sync_remote_dir = "/imported".to_string();
        imported.webdav_sync_retention_days = 1;
        imported.webdav_sync_last_upload_at = None;
        imported.webdav_sync_last_upload_file_name = None;
        imported.webdav_sync_last_download_at = None;
        imported.webdav_sync_last_download_file_name = None;

        let result = resolve_apply_user_config(current, imported, false);

        assert!(result.needs_restart);
        assert!(result.language_changed);
        assert!(result.app_auto_launch_changed);
        assert_eq!(result.config.language, "en");
        assert_eq!(result.config.ws_port, 19599);
        assert!(result.config.webdav_sync_enabled);
        assert_eq!(result.config.webdav_sync_url, "https://webdav.example.com");
        assert_eq!(result.config.webdav_sync_username, "current-user");
        assert_eq!(result.config.webdav_sync_password, "current-password");
        assert_eq!(result.config.webdav_sync_remote_dir, "/current");
        assert_eq!(result.config.webdav_sync_retention_days, 9);
        assert_eq!(
            result.config.webdav_sync_last_upload_file_name.as_deref(),
            Some("upload.json")
        );
        assert_eq!(
            result.config.webdav_sync_last_download_file_name.as_deref(),
            Some("download.json")
        );
    }

    #[test]
    fn sanitize_instance_store_clears_runtime_state() {
        let mut store = InstanceStore::new();
        store.default_settings.last_pid = Some(42);
        store.instances.push(InstanceProfile {
            id: "one".to_string(),
            name: "One".to_string(),
            user_data_dir: "/tmp/one".to_string(),
            working_dir: Some("/tmp".to_string()),
            extra_args: "--flag".to_string(),
            bind_account_id: Some("account".to_string()),
            launch_mode: InstanceLaunchMode::Cli,
            app_speed: Default::default(),
            created_at: 1,
            last_launched_at: Some(2),
            last_pid: Some(3),
        });

        let sanitized = sanitize_instance_store(&store);

        assert_eq!(store.default_settings.last_pid, Some(42));
        assert_eq!(store.instances[0].last_pid, Some(3));
        assert_eq!(store.instances[0].last_launched_at, Some(2));
        assert_eq!(sanitized.default_settings.last_pid, None);
        assert_eq!(sanitized.instances[0].last_pid, None);
        assert_eq!(sanitized.instances[0].last_launched_at, None);
        assert_eq!(sanitized.instances[0].id, "one");
    }
}
