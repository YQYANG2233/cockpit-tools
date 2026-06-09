use std::borrow::Cow;

const PROVIDERS: [(&str, &str); 11] = [
    ("github_copilot", "github-copilot"),
    ("codebuddy_cn", "codebuddy-cn"),
    ("codebuddy", "codebuddy"),
    ("workbuddy", "workbuddy"),
    ("windsurf", "windsurf"),
    ("gemini", "gemini"),
    ("cursor", "cursor"),
    ("qoder", "qoder"),
    ("trae", "trae"),
    ("kiro", "kiro"),
    ("zed", "zed"),
];

pub fn canonical_method(method: &str) -> Cow<'_, str> {
    if method.contains('/') {
        return Cow::Borrowed(method);
    }
    if let Some(alias) = static_alias(method) {
        return Cow::Borrowed(alias);
    }
    if let Some(alias) = provider_alias(method) {
        return Cow::Owned(alias);
    }
    if let Some(alias) = instance_alias(method) {
        return Cow::Owned(alias);
    }
    if let Some(alias) = codex_local_access_alias(method) {
        return Cow::Owned(alias);
    }
    Cow::Borrowed(method)
}

fn static_alias(method: &str) -> Option<&'static str> {
    Some(match method {
        "add_account" => "antigravity/account/add",
        "list_accounts" => "antigravity/account/list",
        "fetch_account_quota" => "antigravity/account/fetch-quota",
        "refresh_current_quota" => "antigravity/account/refresh-current-quota",
        "refresh_all_quotas" => "antigravity/account/refresh-all-quotas",
        "switch_account" => "antigravity/account/switch",
        "delete_account" => "antigravity/account/delete",
        "delete_accounts" => "antigravity/account/delete-many",
        "reorder_accounts" => "antigravity/account/reorder",
        "get_current_account" => "antigravity/account/current/get",
        "set_current_account" => "antigravity/account/current/set",
        "update_account_tags" => "antigravity/account/update-tags",
        "update_account_notes" => "antigravity/account/update-notes",
        "import_from_old_tools" => "antigravity/account/import-old-tools",
        "import_from_local" => "antigravity/account/import-local",
        "import_from_files" => "antigravity/account/import-files",
        "sync_from_extension" => "antigravity/account/sync-extension",
        "sync_current_from_client" => "antigravity/account/sync-current-client",
        "import_from_json" => "antigravity/account/import-json",
        "export_accounts" => "antigravity/account/export",
        "start_oauth_login" => "antigravity/oauth/start",
        "prepare_oauth_url" => "antigravity/oauth/prepare-url",
        "complete_oauth_login" => "antigravity/oauth/complete",
        "submit_oauth_callback_url" => "antigravity/oauth/submit-callback",
        "cancel_oauth_login" => "antigravity/oauth/cancel",
        "load_antigravity_switch_history" => "antigravity/switch-history/load",
        "clear_antigravity_switch_history" => "antigravity/switch-history/clear",
        "get_antigravity_installed_version_info" => "antigravity/runtime/installed-version/get",
        "load_account_groups" => "antigravity/account-groups/load",
        "save_account_groups" => "antigravity/account-groups/save",
        "get_group_settings" => "antigravity/group-settings/get",
        "save_group_settings" => "antigravity/group-settings/save",
        "set_model_group" => "antigravity/group-settings/set-model",
        "remove_model_group" => "antigravity/group-settings/remove-model",
        "set_group_name" => "antigravity/group-settings/set-name",
        "delete_group" => "antigravity/group-settings/delete",
        "update_group_order" => "antigravity/group-settings/update-order",
        "get_display_groups" => "antigravity/group-settings/display-groups/get",

        "list_codex_accounts" => "codex/account/list",
        "get_current_codex_account" => "codex/account/current/get",
        "add_codex_account_with_token" => "codex/account/add-token",
        "add_codex_account_with_api_key" => "codex/account/add-api-key",
        "delete_codex_account" => "codex/account/delete",
        "delete_codex_accounts" => "codex/account/delete-many",
        "import_codex_from_local" => "codex/account/import-local",
        "import_codex_from_json" => "codex/account/import-json",
        "import_codex_from_files" => "codex/account/import-files",
        "start_codex_batch_import_from_files" => "codex/account/batch-import/start-from-files",
        "cancel_codex_batch_import" => "codex/account/batch-import/cancel",
        "resume_codex_batch_import" => "codex/account/batch-import/resume",
        "get_codex_batch_import_preview" => "codex/account/batch-import/preview",
        "confirm_codex_batch_import" => "codex/account/batch-import/confirm",
        "export_codex_accounts" => "codex/account/export",
        "refresh_codex_account_profile" => "codex/account/refresh-profile",
        "refresh_codex_quota" => "codex/account/refresh-quota",
        "refresh_codex_subscription_info" => "codex/account/refresh-subscription-info",
        "refresh_current_codex_quota" => "codex/account/refresh-current-quota",
        "refresh_all_codex_quotas" => "codex/account/refresh-all-quotas",
        "switch_codex_account" => "codex/account/switch",
        "update_codex_account_app_speed" => "codex/account/app-speed/update",
        "update_codex_api_key_credentials" => "codex/account/update-api-key",
        "update_codex_api_key_bound_oauth_account" => "codex/account/update-api-key-bound-oauth",
        "update_codex_account_name" => "codex/account/update-name",
        "update_codex_account_tags" => "codex/account/update-tags",
        "update_codex_account_note" => "codex/account/update-note",
        "load_codex_account_groups" => "codex/account-groups/load",
        "save_codex_account_groups" => "codex/account-groups/save",
        "load_codex_model_providers" => "codex/model-providers/load",
        "save_codex_model_providers" => "codex/model-providers/save",
        "codex_oauth_login_start" => "codex/oauth/start",
        "codex_oauth_login_completed" => "codex/oauth/complete",
        "codex_oauth_login_cancel" => "codex/oauth/cancel",
        "codex_oauth_submit_callback_url" => "codex/oauth/submit-callback",
        "is_codex_oauth_port_in_use" => "codex/oauth/port/in-use",
        "close_codex_oauth_port" => "codex/oauth/port/close",
        "get_codex_config_toml_path" => "codex/config-toml/path",
        "open_codex_config_toml" => "codex/config-toml/open",
        "get_codex_quick_config" => "codex/quick-config/get",
        "save_codex_quick_config" => "codex/quick-config/save",
        "get_codex_app_speed_config" => "codex/app-speed/get",
        "save_codex_app_speed" => "codex/app-speed/save",
        "get_codex_api_service_app_speed_config" => "codex/api-service-app-speed/get",
        "save_codex_api_service_app_speed" => "codex/api-service-app-speed/save",
        "codex_test_model_provider_connection" => "codex/model-provider/connection/test",
        "codex_query_model_provider_usage" => "codex/model-provider/usage/query",
        "codex_get_instance_defaults" => "codex/instance/defaults/get",
        "codex_list_instances" => "codex/instance/list",
        "codex_create_instance" => "codex/instance/create",
        "codex_update_instance" => "codex/instance/update",
        "codex_delete_instance" => "codex/instance/delete",
        "codex_get_instance_quick_config" => "codex/instance/quick-config/get",
        "codex_save_instance_quick_config" => "codex/instance/quick-config/save",
        "codex_open_instance_config_toml" => "codex/instance/config-toml/open",
        "codex_get_instance_launch_command" => "codex/instance/launch-command/get",
        "codex_execute_instance_launch_command" => "codex/instance/launch-command/execute",
        "codex_sync_threads_across_instances" => "codex/instance/threads/sync-all",
        "codex_sync_sessions_to_instance" => "codex/instance/sessions/sync-to-instance",
        "codex_repair_session_visibility_across_instances" => {
            "codex/instance/sessions/visibility/repair"
        }
        "codex_list_sessions_across_instances" => "codex/instance/sessions/list",
        "codex_get_session_token_stats_across_instances" => "codex/instance/sessions/token-stats",
        "codex_move_sessions_to_trash_across_instances" => "codex/instance/sessions/trash",
        "codex_list_trashed_sessions_across_instances" => "codex/instance/sessions/trash/list",
        "codex_restore_sessions_from_trash_across_instances" => {
            "codex/instance/sessions/trash/restore"
        }
        "set_codex_launch_on_switch" => "codex/launch-on-switch/set",
        "set_codex_local_access_entry_visible" => "codex/local-access-entry-visible/set",
        "codex_wakeup_get_cli_status" => "codex/wakeup/cli-status/get",
        "codex_wakeup_update_runtime_config" => "codex/wakeup/runtime-config/update",
        "codex_wakeup_get_overview" => "codex/wakeup/overview/get",
        "codex_wakeup_get_state" => "codex/wakeup/state/get",
        "codex_wakeup_save_state" => "codex/wakeup/state/save",
        "codex_wakeup_load_history" => "codex/wakeup/history/load",
        "codex_wakeup_clear_history" => "codex/wakeup/history/clear",
        "codex_wakeup_cancel_scope" => "codex/wakeup/scope/cancel",
        "codex_wakeup_release_scope" => "codex/wakeup/scope/release",
        "codex_wakeup_test" => "codex/wakeup/test",
        "codex_wakeup_run_task" => "codex/wakeup/task/run",
        "codex_wakeup_run_enabled_tasks" => "codex/wakeup/tasks/run-enabled",

        "get_general_config" => "settings/general/get",
        "save_general_config" => "settings/general/save",
        "get_network_config" => "settings/network/get",
        "save_network_config" => "settings/network/save",
        "get_update_settings" => "settings/update/get",
        "save_update_settings" => "settings/update/save",
        "should_check_updates" => "settings/update/should-check",
        "update_last_check_time" => "settings/update/last-check/update",
        "save_pending_update_notes" => "settings/update/pending-notes/save",
        "check_version_jump" => "settings/update/version-jump/check",
        "get_release_history" => "settings/update/release-history/get",
        "update_log" => "system/log/update",
        "get_update_runtime_info" => "system/update/runtime-info",
        "install_linux_update" => "system/update/linux/install",
        "get_available_terminals" => "system/terminal/list",
        "get_downloads_dir" => "system/downloads-dir/get",
        "get_home_dir" => "system/home-dir/get",
        "save_text_file" => "system/text-file/save",
        "read_text_file" => "system/text-file/read",
        "open_path" => "system/path/open",
        "open_data_folder" => "system/data-folder/open",
        "open_folder" => "system/folder/open",
        "set_app_path" => "system/app-path/set",
        "detect_app_path" => "system/app-path/detect",
        "set_wakeup_override" => "system/wakeup-override/set",
        "delete_corrupted_file" => "system/corrupted-file/delete",
        "external_import_take_pending" => "external-import/pending/take",
        "external_import_submit_url" => "external-import/pending/submit-url",
        "external_import_fetch_import_url" => "external-import/fetch-url",
        "handle_window_close" => "desktop-shell/window/close",
        "show_floating_card_window" => "desktop-shell/floating-card/show",
        "show_instance_floating_card_window" => "desktop-shell/floating-card/show-instance",
        "get_floating_card_context" => "desktop-shell/floating-card/context/get",
        "hide_floating_card_window" => "desktop-shell/floating-card/hide",
        "hide_current_floating_card_window" => "desktop-shell/floating-card/hide-current",
        "set_floating_card_always_on_top" => "desktop-shell/floating-card/always-on-top/set",
        "set_current_floating_card_window_always_on_top" => {
            "desktop-shell/floating-card/current-always-on-top/set"
        }
        "set_floating_card_confirm_on_close" => "desktop-shell/floating-card/confirm-on-close/set",
        "save_floating_card_position" => "desktop-shell/floating-card/position/save",
        "show_main_window_and_navigate" => "desktop-shell/main-window/navigate",
        "save_tray_platform_layout" => "desktop-shell/tray-layout/save",
        "logs_get_snapshot" => "logs/snapshot/get",
        "logs_open_log_directory" => "logs/directory/open",
        "get_auto_backup_settings" => "backup/auto/settings/get",
        "save_auto_backup_settings" => "backup/auto/settings/save",
        "update_auto_backup_last_run" => "backup/auto/last-run/update",
        "write_auto_backup_file" => "backup/auto/file/write",
        "read_auto_backup_file" => "backup/auto/file/read",
        "copy_auto_backup_file" => "backup/auto/file/copy",
        "delete_auto_backup_file" => "backup/auto/file/delete",
        "list_auto_backup_files" => "backup/auto/files/list",
        "cleanup_auto_backup_files" => "backup/auto/files/cleanup",
        "open_auto_backup_dir" => "backup/auto/directory/open",
        "data_transfer_get_user_config" => "data-transfer/user-config/get",
        "data_transfer_apply_user_config" => "data-transfer/user-config/apply",
        "data_transfer_get_instance_store" => "data-transfer/instance-store/get",
        "data_transfer_replace_instance_store" => "data-transfer/instance-store/replace",
        "get_webdav_sync_settings" => "webdav/settings/get",
        "save_webdav_sync_settings" => "webdav/settings/save",
        "test_webdav_sync_connection" => "webdav/connection/test",
        "upload_auto_backup_to_webdav" => "webdav/backup/upload-auto",
        "list_webdav_backup_files" => "webdav/backup/files/list",
        "read_webdav_backup_file" => "webdav/backup/file/read",
        "delete_webdav_backup_file" => "webdav/backup/file/delete",
        "get_provider_current_account_id" => "provider/current-account/get",
        "announcement_get_state" => "announcement/state/get",
        "announcement_force_refresh" => "announcement/state/force-refresh",
        "announcement_mark_as_read" => "announcement/read/mark",
        "announcement_mark_all_as_read" => "announcement/read/mark-all",
        "announcement_get_top_right_ad" => "announcement/top-right-ad/get",
        "announcement_get_sponsor_module" => "announcement/sponsor-module/get",
        "announcement_force_refresh_sponsor_module" => "announcement/sponsor-module/force-refresh",
        "wakeup_ensure_runtime_ready" => "wakeup/runtime/ensure-ready",
        "trigger_wakeup" => "wakeup/trigger",
        "fetch_available_models" => "wakeup/models/list",
        "wakeup_validate_crontab" => "wakeup/crontab/validate",
        "wakeup_sync_state" => "wakeup/state/sync",
        "wakeup_run_enabled_tasks" => "wakeup/tasks/run-enabled",
        "wakeup_load_history" => "wakeup/history/load",
        "wakeup_add_history" => "wakeup/history/add",
        "wakeup_clear_history" => "wakeup/history/clear",
        "wakeup_cancel_scope" => "wakeup/scope/cancel",
        "wakeup_release_scope" => "wakeup/scope/release",
        "wakeup_verification_load_state" => "wakeup/verification/state/load",
        "wakeup_verification_load_history" => "wakeup/verification/history/load",
        "wakeup_verification_delete_history" => "wakeup/verification/history/delete",
        "wakeup_verification_run_batch" => "wakeup/verification/batch/run",
        "wakeup_set_official_ls_version_mode" => "wakeup/official-ls-version-mode/set",
        "confirm_wakeup_task" => "wakeup/task/confirm",
        "cancel_wakeup_task" => "wakeup/task/cancel",
        "check_wakeup_timeouts" => "wakeup/timeouts/check",
        "zed_get_runtime_status" => "zed/runtime/status",
        "zed_start_default_session" => "zed/runtime/default/start",
        "zed_stop_default_session" => "zed/runtime/default/stop",
        "zed_restart_default_session" => "zed/runtime/default/restart",
        "zed_focus_default_session" => "zed/runtime/default/focus",
        _ => return None,
    })
}

fn provider_alias(method: &str) -> Option<String> {
    for (legacy, slug) in PROVIDERS {
        let exact = |suffix: &str| method == format!("{legacy}{suffix}");
        let prefixed = |prefix: &str, suffix: &str| method == format!("{prefix}{legacy}{suffix}");

        if prefixed("list_", "_accounts") {
            return Some(format!("{slug}/account/list"));
        }
        if exact("_oauth_login_start") {
            return Some(format!("{slug}/oauth/start"));
        }
        if exact("_oauth_login_peek") {
            return Some(format!("{slug}/oauth/peek"));
        }
        if exact("_oauth_login_complete") {
            return Some(format!("{slug}/oauth/complete"));
        }
        if exact("_oauth_login_cancel") {
            return Some(format!("{slug}/oauth/cancel"));
        }
        if exact("_oauth_submit_callback_url") {
            return Some(format!("{slug}/oauth/submit-callback"));
        }
        if prefixed("add_", "_account_with_token") {
            return Some(format!("{slug}/account/add-token"));
        }
        if method == "add_windsurf_account_with_password" {
            return Some("windsurf/account/add-password".to_string());
        }
        if method == "add_windsurf_accounts_with_password" {
            return Some("windsurf/account/add-password-batch".to_string());
        }
        if prefixed("inject_", "_account") || prefixed("inject_", "_to_vscode") {
            return Some(format!("{slug}/account/inject"));
        }
        if prefixed("delete_", "_account") {
            return Some(format!("{slug}/account/delete"));
        }
        if prefixed("delete_", "_accounts") {
            return Some(format!("{slug}/account/delete-many"));
        }
        if prefixed("import_", "_from_json") {
            return Some(format!("{slug}/account/import-json"));
        }
        if prefixed("import_", "_from_local") {
            return Some(format!("{slug}/account/import-local"));
        }
        if prefixed("export_", "_accounts") {
            return Some(format!("{slug}/account/export"));
        }
        if prefixed("refresh_", "_token") {
            return Some(format!("{slug}/account/refresh"));
        }
        if prefixed("refresh_all_", "_tokens") {
            return Some(format!("{slug}/account/refresh-all"));
        }
        if prefixed("update_", "_account_tags") {
            return Some(format!("{slug}/account/update-tags"));
        }
        if prefixed("get_", "_accounts_index_path") {
            return Some(format!("{slug}/account/index-path"));
        }
    }

    match method {
        "sync_codebuddy_cn_to_workbuddy" => {
            Some("codebuddy-cn/account/sync-to-workbuddy".to_string())
        }
        "sync_workbuddy_to_codebuddy_cn" => {
            Some("workbuddy/account/sync-to-codebuddy-cn".to_string())
        }
        "zed_logout_current_account" => Some("zed/account/logout-current".to_string()),
        "get_checkin_status_workbuddy" => Some("workbuddy/checkin/status".to_string()),
        "checkin_workbuddy" => Some("workbuddy/checkin/run".to_string()),
        _ => None,
    }
}

fn instance_alias(method: &str) -> Option<String> {
    let instance_methods = [
        ("get_instance_defaults", "instance/defaults/get"),
        ("list_instances", "instance/list"),
        ("create_instance", "instance/create"),
        ("update_instance", "instance/update"),
        ("delete_instance", "instance/delete"),
        ("start_instance", "instance/start"),
        ("stop_instance", "instance/stop"),
        ("open_instance_window", "instance/window/open"),
        ("close_all_instances", "instance/close-all"),
    ];

    for (suffix, rpc_suffix) in instance_methods {
        if method == suffix {
            return Some(format!("antigravity/{rpc_suffix}"));
        }
        if method == format!("codex_{suffix}") {
            return Some(format!("codex/{rpc_suffix}"));
        }
        for (legacy, slug) in PROVIDERS {
            if method == format!("{legacy}_{suffix}") {
                return Some(format!("{slug}/{rpc_suffix}"));
            }
        }
    }

    match method {
        "gemini_get_instance_launch_command" => {
            Some("gemini/instance/launch-command/get".to_string())
        }
        "gemini_execute_instance_launch_command" => {
            Some("gemini/instance/launch-command/execute".to_string())
        }
        _ => None,
    }
}

fn codex_local_access_alias(method: &str) -> Option<String> {
    method
        .strip_prefix("codex_local_access_")
        .map(|suffix| format!("codex/local-access/{}", suffix.replace('_', "-")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("cockpit-service must live under crates/")
            .to_path_buf()
    }

    fn collect_frontend_sources(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if file_name == "node_modules" || file_name == "dist" {
                continue;
            }
            if path.is_dir() {
                collect_frontend_sources(&path, out);
                continue;
            }
            let is_frontend_source = matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("ts" | "tsx")
            );
            if is_frontend_source {
                out.push(path);
            }
        }
    }

    fn is_ident_char(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
    }

    fn extract_static_invoke_commands(source: &str) -> Vec<String> {
        let bytes = source.as_bytes();
        let mut commands = Vec::new();
        let mut offset = 0usize;
        while let Some(found) = source[offset..].find("invoke") {
            let start = offset + found;
            let prev_is_ident = start
                .checked_sub(1)
                .and_then(|idx| bytes.get(idx))
                .map(|byte| is_ident_char(*byte))
                .unwrap_or(false);
            let next_is_ident = bytes
                .get(start + "invoke".len())
                .map(|byte| is_ident_char(*byte))
                .unwrap_or(false);
            if prev_is_ident || next_is_ident {
                offset = start + "invoke".len();
                continue;
            }

            let mut cursor = start + "invoke".len();
            while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'<') {
                let mut depth = 1usize;
                cursor += 1;
                while let Some(byte) = bytes.get(cursor) {
                    match byte {
                        b'<' => depth += 1,
                        b'>' => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                cursor += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    cursor += 1;
                }
                while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
                    cursor += 1;
                }
            }
            if bytes.get(cursor) != Some(&b'(') {
                offset = cursor;
                continue;
            }
            cursor += 1;
            while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
                cursor += 1;
            }
            let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() else {
                offset = cursor;
                continue;
            };
            cursor += 1;
            let command_start = cursor;
            while let Some(byte) = bytes.get(cursor) {
                if *byte == quote && bytes.get(cursor.saturating_sub(1)) != Some(&b'\\') {
                    commands.push(source[command_start..cursor].to_string());
                    cursor += 1;
                    break;
                }
                cursor += 1;
            }
            offset = cursor;
        }
        commands
    }

    fn extract_tauri_registered_commands(source: &str) -> Vec<String> {
        let Some(start) = source.find("tauri::generate_handler![") else {
            return Vec::new();
        };
        let body_start = start + "tauri::generate_handler![".len();
        let mut depth = 1usize;
        let mut body_end = body_start;
        for (offset, ch) in source[body_start..].char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        body_end = body_start + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        if body_end <= body_start {
            return Vec::new();
        }

        source[body_start..body_end]
            .lines()
            .filter_map(|line| {
                let line = line.split("//").next().unwrap_or("").trim();
                let entry = line.strip_prefix("commands::")?.trim_end_matches(',');
                entry.rsplit("::").next().map(str::trim).and_then(|name| {
                    (!name.is_empty()
                        && name
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'))
                    .then(|| name.to_string())
                })
            })
            .collect()
    }

    #[test]
    fn tauri_registered_commands_are_rpc_mapped() {
        let root = workspace_root();
        let lib_rs = root.join("src-tauri").join("src").join("lib.rs");
        let source = fs::read_to_string(&lib_rs)
            .unwrap_or_else(|err| panic!("read {} failed: {err}", lib_rs.display()));
        let commands = extract_tauri_registered_commands(&source);
        assert!(
            commands.len() > 250,
            "registered Tauri command scan found suspiciously few commands: {}",
            commands.len()
        );

        let mut unmapped = Vec::new();
        for command in commands {
            if canonical_method(&command) == command.as_str() {
                unmapped.push(command);
            }
        }
        assert!(
            unmapped.is_empty(),
            "Tauri registered commands missing RPC catalog mappings:\n{}",
            unmapped.join("\n")
        );
    }

    #[test]
    fn maps_legacy_account_command() {
        assert_eq!(
            canonical_method("list_accounts"),
            "antigravity/account/list"
        );
    }

    #[test]
    fn maps_provider_command_by_rule() {
        assert_eq!(
            canonical_method("import_github_copilot_from_local"),
            "github-copilot/account/import-local"
        );
        assert_eq!(
            canonical_method("codebuddy_cn_oauth_login_complete"),
            "codebuddy-cn/oauth/complete"
        );
    }

    #[test]
    fn maps_codex_speed_and_model_provider_commands() {
        assert_eq!(
            canonical_method("get_codex_app_speed_config"),
            "codex/app-speed/get"
        );
        assert_eq!(
            canonical_method("codex_test_model_provider_connection"),
            "codex/model-provider/connection/test"
        );
        assert_eq!(
            canonical_method("codex_query_model_provider_usage"),
            "codex/model-provider/usage/query"
        );
    }

    #[test]
    fn maps_codex_batch_import_commands() {
        assert_eq!(
            canonical_method("start_codex_batch_import_from_files"),
            "codex/account/batch-import/start-from-files"
        );
        assert_eq!(
            canonical_method("confirm_codex_batch_import"),
            "codex/account/batch-import/confirm"
        );
    }

    #[test]
    fn maps_zed_runtime_commands() {
        assert_eq!(
            canonical_method("zed_get_runtime_status"),
            "zed/runtime/status"
        );
        assert_eq!(
            canonical_method("zed_start_default_session"),
            "zed/runtime/default/start"
        );
        assert_eq!(
            canonical_method("zed_focus_default_session"),
            "zed/runtime/default/focus"
        );
    }

    #[test]
    fn maps_instance_commands_by_rule() {
        assert_eq!(
            canonical_method("start_instance"),
            "antigravity/instance/start"
        );
        assert_eq!(
            canonical_method("github_copilot_list_instances"),
            "github-copilot/instance/list"
        );
        assert_eq!(
            canonical_method("codebuddy_cn_start_instance"),
            "codebuddy-cn/instance/start"
        );
        assert_eq!(
            canonical_method("gemini_execute_instance_launch_command"),
            "gemini/instance/launch-command/execute"
        );
        assert_eq!(
            canonical_method("codex_get_instance_launch_command"),
            "codex/instance/launch-command/get"
        );
        assert_eq!(
            canonical_method("codex_start_instance"),
            "codex/instance/start"
        );
    }

    #[test]
    fn maps_wakeup_and_session_commands() {
        assert_eq!(
            canonical_method("codex_wakeup_get_overview"),
            "codex/wakeup/overview/get"
        );
        assert_eq!(canonical_method("codex_wakeup_test"), "codex/wakeup/test");
        assert_eq!(
            canonical_method("codex_wakeup_run_task"),
            "codex/wakeup/task/run"
        );
        assert_eq!(
            canonical_method("codex_wakeup_run_enabled_tasks"),
            "codex/wakeup/tasks/run-enabled"
        );
        assert_eq!(
            canonical_method("codex_sync_threads_across_instances"),
            "codex/instance/threads/sync-all"
        );
        assert_eq!(canonical_method("wakeup_sync_state"), "wakeup/state/sync");
        assert_eq!(
            canonical_method("check_wakeup_timeouts"),
            "wakeup/timeouts/check"
        );
        assert_eq!(
            canonical_method("wakeup_cancel_scope"),
            "wakeup/scope/cancel"
        );
        assert_eq!(
            canonical_method("wakeup_release_scope"),
            "wakeup/scope/release"
        );
        assert_eq!(
            canonical_method("wakeup_verification_load_state"),
            "wakeup/verification/state/load"
        );
        assert_eq!(
            canonical_method("wakeup_verification_run_batch"),
            "wakeup/verification/batch/run"
        );
    }

    #[test]
    fn maps_misc_frontend_commands() {
        assert_eq!(
            canonical_method("get_antigravity_installed_version_info"),
            "antigravity/runtime/installed-version/get"
        );
        assert_eq!(
            canonical_method("refresh_codex_subscription_info"),
            "codex/account/refresh-subscription-info"
        );
        assert_eq!(
            canonical_method("checkin_workbuddy"),
            "workbuddy/checkin/run"
        );
    }

    #[test]
    fn keeps_canonical_method() {
        assert_eq!(
            canonical_method("settings/general/get"),
            "settings/general/get"
        );
    }

    #[test]
    fn frontend_static_invokes_are_rpc_mapped() {
        let root = workspace_root();
        let mut sources = Vec::new();
        collect_frontend_sources(&root.join("src"), &mut sources);
        assert!(
            !sources.is_empty(),
            "expected frontend sources under {}",
            root.join("src").display()
        );

        let mut unmapped = Vec::new();
        let mut static_command_count = 0usize;
        for source_path in sources {
            let source = fs::read_to_string(&source_path)
                .unwrap_or_else(|err| panic!("read {} failed: {err}", source_path.display()));
            for command in extract_static_invoke_commands(&source) {
                static_command_count += 1;
                if !command.contains('/') && canonical_method(&command) == command.as_str() {
                    unmapped.push(format!(
                        "{} -> {}",
                        source_path
                            .strip_prefix(&root)
                            .unwrap_or(&source_path)
                            .display(),
                        command
                    ));
                }
            }
        }

        assert!(
            static_command_count > 200,
            "static invoke scan found suspiciously few commands: {static_command_count}"
        );
        assert!(
            unmapped.is_empty(),
            "frontend invoke commands missing RPC catalog mappings:\n{}",
            unmapped.join("\n")
        );
    }

    #[test]
    fn frontend_keeps_tauri_api_behind_runtime_transport() {
        let root = workspace_root();
        let mut sources = Vec::new();
        collect_frontend_sources(&root.join("src"), &mut sources);
        let allowed_tauri_import = Path::new("src")
            .join("lib")
            .join("runtime")
            .join("invoke.ts");
        let mut violations = Vec::new();

        for source_path in sources {
            let source = fs::read_to_string(&source_path)
                .unwrap_or_else(|err| panic!("read {} failed: {err}", source_path.display()));
            let relative = source_path
                .strip_prefix(&root)
                .unwrap_or(&source_path)
                .to_path_buf();
            if source.contains("@tauri-apps/api/core") && relative != allowed_tauri_import {
                violations.push(format!(
                    "{} imports @tauri-apps/api/core directly",
                    relative.display()
                ));
            }
            if source.contains("__TAURI_INTERNALS__") || source.contains("webBridge") {
                violations.push(format!(
                    "{} contains forbidden browser-side Tauri bridge/shim marker",
                    relative.display()
                ));
            }
        }

        assert!(
            violations.is_empty(),
            "frontend runtime architecture violations:\n{}",
            violations.join("\n")
        );
    }
}
