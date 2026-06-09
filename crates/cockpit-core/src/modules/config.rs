//! 配置服务模块
//! 管理应用配置，包括 WebSocket 端口等

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

/// 默认 WebSocket 端口
pub const DEFAULT_WS_PORT: u16 = 19528;
/// 默认网页查询服务端口
pub const DEFAULT_REPORT_PORT: u16 = 18081;

/// 端口尝试范围（从配置端口开始，最多尝试 100 个）
pub const PORT_RANGE: u16 = 100;

/// 服务状态配置文件名（供外部客户端读取）
const SERVER_STATUS_FILE: &str = "server.json";

/// 用户配置文件名
const USER_CONFIG_FILE: &str = "config.json";

/// 数据目录名
const DATA_DIR: &str = ".antigravity_cockpit";

/// 服务状态（写入共享文件供其他客户端读取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    /// WebSocket 服务端口（实际绑定的端口）
    pub ws_port: u16,
    /// 服务版本
    pub version: String,
    /// 进程 ID（用于检测服务是否存活）
    pub pid: u32,
    /// 启动时间戳
    pub started_at: i64,
}

/// 用户配置（持久化存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub ws_enabled: bool,
    pub ws_port: u16,
    pub actual_port: Option<u16>,
    pub default_port: u16,
    pub report_enabled: bool,
    pub report_port: u16,
    pub report_actual_port: Option<u16>,
    pub report_default_port: u16,
    pub report_token: String,
    pub global_proxy_enabled: bool,
    pub global_proxy_url: String,
    pub global_proxy_no_proxy: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkConfigUpdate {
    pub ws_enabled: bool,
    pub ws_port: u16,
    pub report_enabled: Option<bool>,
    pub report_port: Option<u16>,
    pub report_token: Option<String>,
    pub global_proxy_enabled: Option<bool>,
    pub global_proxy_url: Option<String>,
    pub global_proxy_no_proxy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub language: String,
    pub default_terminal: String,
    pub theme: String,
    pub ui_scale: f64,
    pub auto_refresh_minutes: i32,
    pub codex_auto_refresh_minutes: i32,
    pub codex_sync_wsl: bool,
    pub codex_wsl_config_dir: String,
    pub zed_auto_refresh_minutes: i32,
    pub ghcp_auto_refresh_minutes: i32,
    pub windsurf_auto_refresh_minutes: i32,
    pub kiro_auto_refresh_minutes: i32,
    pub cursor_auto_refresh_minutes: i32,
    pub gemini_auto_refresh_minutes: i32,
    pub gemini_sync_wsl: bool,
    pub codebuddy_auto_refresh_minutes: i32,
    pub codebuddy_cn_auto_refresh_minutes: i32,
    pub workbuddy_auto_refresh_minutes: i32,
    pub qoder_auto_refresh_minutes: i32,
    pub trae_auto_refresh_minutes: i32,
    pub close_behavior: String,
    pub minimize_behavior: String,
    pub hide_dock_icon: bool,
    pub tray_icon_style: String,
    pub floating_card_show_on_startup: bool,
    pub floating_card_always_on_top: bool,
    pub app_auto_launch_enabled: bool,
    pub antigravity_startup_wakeup_enabled: bool,
    pub antigravity_startup_wakeup_delay_seconds: i32,
    pub codex_startup_wakeup_enabled: bool,
    pub codex_startup_wakeup_delay_seconds: i32,
    pub floating_card_confirm_on_close: bool,
    pub opencode_app_path: String,
    pub antigravity_app_path: String,
    pub codex_app_path: String,
    pub codex_specified_app_path: String,
    pub zed_app_path: String,
    pub vscode_app_path: String,
    pub windsurf_app_path: String,
    pub kiro_app_path: String,
    pub cursor_app_path: String,
    pub codebuddy_app_path: String,
    pub codebuddy_cn_app_path: String,
    pub qoder_app_path: String,
    pub trae_app_path: String,
    pub workbuddy_app_path: String,
    pub opencode_sync_on_switch: bool,
    pub opencode_auth_overwrite_on_switch: bool,
    pub ghcp_opencode_sync_on_switch: bool,
    pub ghcp_opencode_auth_overwrite_on_switch: bool,
    pub ghcp_launch_on_switch: bool,
    pub openclaw_auth_overwrite_on_switch: bool,
    pub codex_launch_on_switch: bool,
    pub codex_restart_specified_app_on_switch: bool,
    pub codex_local_access_entry_visible: bool,
    pub top_right_ad_visible: bool,
    pub antigravity_dual_switch_no_restart_enabled: bool,
    pub auto_switch_enabled: bool,
    pub auto_switch_threshold: i32,
    pub auto_switch_credits_enabled: bool,
    pub auto_switch_credits_threshold: i32,
    pub auto_switch_scope_mode: String,
    pub auto_switch_selected_group_ids: Vec<String>,
    pub auto_switch_account_scope_mode: String,
    pub auto_switch_selected_account_ids: Vec<String>,
    pub codex_auto_switch_enabled: bool,
    pub codex_auto_switch_primary_threshold: i32,
    pub codex_auto_switch_secondary_threshold: i32,
    pub codex_auto_switch_account_scope_mode: String,
    pub codex_auto_switch_selected_account_ids: Vec<String>,
    pub quota_alert_enabled: bool,
    pub quota_alert_threshold: i32,
    pub codex_quota_alert_enabled: bool,
    pub codex_quota_alert_threshold: i32,
    pub zed_quota_alert_enabled: bool,
    pub zed_quota_alert_threshold: i32,
    pub codex_quota_alert_primary_threshold: i32,
    pub codex_quota_alert_secondary_threshold: i32,
    pub ghcp_quota_alert_enabled: bool,
    pub ghcp_quota_alert_threshold: i32,
    pub windsurf_quota_alert_enabled: bool,
    pub windsurf_quota_alert_threshold: i32,
    pub kiro_quota_alert_enabled: bool,
    pub kiro_quota_alert_threshold: i32,
    pub cursor_quota_alert_enabled: bool,
    pub cursor_quota_alert_threshold: i32,
    pub gemini_quota_alert_enabled: bool,
    pub gemini_quota_alert_threshold: i32,
    pub codebuddy_quota_alert_enabled: bool,
    pub codebuddy_quota_alert_threshold: i32,
    pub codebuddy_cn_quota_alert_enabled: bool,
    pub codebuddy_cn_quota_alert_threshold: i32,
    pub qoder_quota_alert_enabled: bool,
    pub qoder_quota_alert_threshold: i32,
    pub trae_quota_alert_enabled: bool,
    pub trae_quota_alert_threshold: i32,
    pub workbuddy_quota_alert_enabled: bool,
    pub workbuddy_quota_alert_threshold: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfigSaveInput {
    pub language: String,
    pub default_terminal: Option<String>,
    pub theme: String,
    pub ui_scale: Option<f64>,
    pub auto_refresh_minutes: i32,
    pub codex_auto_refresh_minutes: i32,
    pub codex_sync_wsl: Option<bool>,
    pub codex_wsl_config_dir: Option<String>,
    pub zed_auto_refresh_minutes: Option<i32>,
    pub ghcp_auto_refresh_minutes: Option<i32>,
    pub windsurf_auto_refresh_minutes: Option<i32>,
    pub kiro_auto_refresh_minutes: Option<i32>,
    pub cursor_auto_refresh_minutes: Option<i32>,
    pub gemini_auto_refresh_minutes: Option<i32>,
    pub gemini_sync_wsl: Option<bool>,
    pub codebuddy_auto_refresh_minutes: Option<i32>,
    pub codebuddy_cn_auto_refresh_minutes: Option<i32>,
    pub workbuddy_auto_refresh_minutes: Option<i32>,
    pub qoder_auto_refresh_minutes: Option<i32>,
    pub trae_auto_refresh_minutes: Option<i32>,
    pub close_behavior: String,
    pub minimize_behavior: Option<String>,
    pub hide_dock_icon: Option<bool>,
    pub tray_icon_style: Option<String>,
    pub floating_card_show_on_startup: Option<bool>,
    pub floating_card_always_on_top: Option<bool>,
    pub app_auto_launch_enabled: Option<bool>,
    pub antigravity_startup_wakeup_enabled: Option<bool>,
    pub antigravity_startup_wakeup_delay_seconds: Option<i32>,
    pub codex_startup_wakeup_enabled: Option<bool>,
    pub codex_startup_wakeup_delay_seconds: Option<i32>,
    pub floating_card_confirm_on_close: Option<bool>,
    pub opencode_app_path: String,
    pub antigravity_app_path: String,
    pub codex_app_path: String,
    pub codex_specified_app_path: Option<String>,
    pub zed_app_path: Option<String>,
    pub vscode_app_path: String,
    pub windsurf_app_path: Option<String>,
    pub kiro_app_path: Option<String>,
    pub cursor_app_path: Option<String>,
    pub codebuddy_app_path: Option<String>,
    pub codebuddy_cn_app_path: Option<String>,
    pub qoder_app_path: Option<String>,
    pub trae_app_path: Option<String>,
    pub workbuddy_app_path: Option<String>,
    pub opencode_sync_on_switch: bool,
    pub opencode_auth_overwrite_on_switch: Option<bool>,
    pub ghcp_opencode_sync_on_switch: Option<bool>,
    pub ghcp_opencode_auth_overwrite_on_switch: Option<bool>,
    pub ghcp_launch_on_switch: Option<bool>,
    pub openclaw_auth_overwrite_on_switch: Option<bool>,
    pub codex_launch_on_switch: bool,
    pub codex_restart_specified_app_on_switch: Option<bool>,
    pub codex_local_access_entry_visible: Option<bool>,
    pub top_right_ad_visible: Option<bool>,
    pub antigravity_dual_switch_no_restart_enabled: Option<bool>,
    pub auto_switch_enabled: Option<bool>,
    pub auto_switch_threshold: Option<i32>,
    pub auto_switch_credits_enabled: Option<bool>,
    pub auto_switch_credits_threshold: Option<i32>,
    pub auto_switch_scope_mode: Option<String>,
    pub auto_switch_selected_group_ids: Option<Vec<String>>,
    pub auto_switch_account_scope_mode: Option<String>,
    pub auto_switch_selected_account_ids: Option<Vec<String>>,
    pub codex_auto_switch_enabled: Option<bool>,
    pub codex_auto_switch_primary_threshold: Option<i32>,
    pub codex_auto_switch_secondary_threshold: Option<i32>,
    pub codex_auto_switch_account_scope_mode: Option<String>,
    pub codex_auto_switch_selected_account_ids: Option<Vec<String>>,
    pub quota_alert_enabled: Option<bool>,
    pub quota_alert_threshold: Option<i32>,
    pub codex_quota_alert_enabled: Option<bool>,
    pub codex_quota_alert_threshold: Option<i32>,
    pub zed_quota_alert_enabled: Option<bool>,
    pub zed_quota_alert_threshold: Option<i32>,
    pub codex_quota_alert_primary_threshold: Option<i32>,
    pub codex_quota_alert_secondary_threshold: Option<i32>,
    pub ghcp_quota_alert_enabled: Option<bool>,
    pub ghcp_quota_alert_threshold: Option<i32>,
    pub windsurf_quota_alert_enabled: Option<bool>,
    pub windsurf_quota_alert_threshold: Option<i32>,
    pub kiro_quota_alert_enabled: Option<bool>,
    pub kiro_quota_alert_threshold: Option<i32>,
    pub cursor_quota_alert_enabled: Option<bool>,
    pub cursor_quota_alert_threshold: Option<i32>,
    pub gemini_quota_alert_enabled: Option<bool>,
    pub gemini_quota_alert_threshold: Option<i32>,
    pub codebuddy_quota_alert_enabled: Option<bool>,
    pub codebuddy_quota_alert_threshold: Option<i32>,
    pub codebuddy_cn_quota_alert_enabled: Option<bool>,
    pub codebuddy_cn_quota_alert_threshold: Option<i32>,
    pub qoder_quota_alert_enabled: Option<bool>,
    pub qoder_quota_alert_threshold: Option<i32>,
    pub trae_quota_alert_enabled: Option<bool>,
    pub trae_quota_alert_threshold: Option<i32>,
    pub workbuddy_quota_alert_enabled: Option<bool>,
    pub workbuddy_quota_alert_threshold: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    /// WebSocket 服务是否启用
    #[serde(default = "default_ws_enabled")]
    pub ws_enabled: bool,
    /// WebSocket 首选端口（用户配置的，实际可能不同）
    #[serde(default = "default_ws_port")]
    pub ws_port: u16,
    /// 网页查询服务是否启用
    #[serde(default = "default_report_enabled")]
    pub report_enabled: bool,
    /// 网页查询服务首选端口
    #[serde(default = "default_report_port")]
    pub report_port: u16,
    /// 网页查询服务访问令牌
    #[serde(default = "default_report_token")]
    pub report_token: String,
    /// 全局代理开关（仅对受管启动链路生效）
    #[serde(default = "default_global_proxy_enabled")]
    pub global_proxy_enabled: bool,
    /// 全局代理地址（如 http://127.0.0.1:7890）
    #[serde(default = "default_global_proxy_url")]
    pub global_proxy_url: String,
    /// NO_PROXY 白名单（逗号分隔）
    #[serde(default = "default_global_proxy_no_proxy")]
    pub global_proxy_no_proxy: String,
    /// 界面语言
    #[serde(default = "default_language")]
    pub language: String,
    /// 默认终端
    #[serde(default = "default_default_terminal")]
    pub default_terminal: String,
    /// 应用主题
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 界面缩放比例
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f64,
    /// 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_auto_refresh")]
    pub auto_refresh_minutes: i32,
    /// Codex 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_codex_auto_refresh")]
    pub codex_auto_refresh_minutes: i32,
    /// Codex 切号时是否同步覆盖 WSL 配置 (Windows Only)
    #[serde(default = "default_codex_sync_wsl")]
    pub codex_sync_wsl: bool,
    /// Codex WSL 配置目录 (Windows Only)
    #[serde(default = "default_codex_wsl_config_dir")]
    pub codex_wsl_config_dir: String,
    /// Zed 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_zed_auto_refresh")]
    pub zed_auto_refresh_minutes: i32,
    /// GitHub Copilot 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_ghcp_auto_refresh")]
    pub ghcp_auto_refresh_minutes: i32,
    /// Windsurf 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_windsurf_auto_refresh")]
    pub windsurf_auto_refresh_minutes: i32,
    /// Kiro 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_kiro_auto_refresh")]
    pub kiro_auto_refresh_minutes: i32,
    /// Cursor 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_cursor_auto_refresh")]
    pub cursor_auto_refresh_minutes: i32,
    /// Gemini 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_gemini_auto_refresh")]
    pub gemini_auto_refresh_minutes: i32,
    /// Gemini 切号时是否同步覆盖 WSL 配置 (Windows Only)
    #[serde(default = "default_gemini_sync_wsl")]
    pub gemini_sync_wsl: bool,
    /// CodeBuddy 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_codebuddy_auto_refresh")]
    pub codebuddy_auto_refresh_minutes: i32,
    /// CodeBuddy CN 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_codebuddy_cn_auto_refresh")]
    pub codebuddy_cn_auto_refresh_minutes: i32,
    /// WorkBuddy 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_workbuddy_auto_refresh")]
    pub workbuddy_auto_refresh_minutes: i32,
    /// Qoder 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_qoder_auto_refresh")]
    pub qoder_auto_refresh_minutes: i32,
    /// Trae 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_trae_auto_refresh")]
    pub trae_auto_refresh_minutes: i32,
    /// 窗口关闭行为
    #[serde(default = "default_close_behavior")]
    pub close_behavior: CloseWindowBehavior,
    /// 窗口最小化行为（macOS）
    #[serde(default = "default_minimize_behavior")]
    pub minimize_behavior: MinimizeWindowBehavior,
    /// 是否隐藏 Dock 图标（macOS）
    #[serde(default = "default_hide_dock_icon")]
    pub hide_dock_icon: bool,
    #[serde(default = "default_tray_icon_style")]
    pub tray_icon_style: TrayIconStyle,
    /// 是否在启动后自动显示悬浮卡片
    #[serde(default = "default_floating_card_show_on_startup")]
    pub floating_card_show_on_startup: bool,
    /// 悬浮卡片是否默认置顶
    #[serde(default = "default_floating_card_always_on_top")]
    pub floating_card_always_on_top: bool,
    /// 是否启用应用开机自启动
    #[serde(default = "default_app_auto_launch_enabled")]
    pub app_auto_launch_enabled: bool,
    /// 是否在应用启动后触发 Antigravity IDE 唤醒
    #[serde(default = "default_antigravity_startup_wakeup_enabled")]
    pub antigravity_startup_wakeup_enabled: bool,
    /// Antigravity IDE 启动后唤醒延时（秒），0 表示立即
    #[serde(default = "default_antigravity_startup_wakeup_delay_seconds")]
    pub antigravity_startup_wakeup_delay_seconds: i32,
    /// 是否在应用启动后触发 Codex 唤醒
    #[serde(default = "default_codex_startup_wakeup_enabled")]
    pub codex_startup_wakeup_enabled: bool,
    /// Codex 启动后唤醒延时（秒），0 表示立即
    #[serde(default = "default_codex_startup_wakeup_delay_seconds")]
    pub codex_startup_wakeup_delay_seconds: i32,
    /// 关闭悬浮卡片前是否显示确认弹框
    #[serde(default = "default_floating_card_confirm_on_close")]
    pub floating_card_confirm_on_close: bool,
    /// 是否启用定期自动备份
    #[serde(default = "default_auto_backup_enabled")]
    pub auto_backup_enabled: bool,
    /// 自动备份是否包含账号数据
    #[serde(default = "default_auto_backup_include_accounts")]
    pub auto_backup_include_accounts: bool,
    /// 自动备份是否包含配置数据
    #[serde(default = "default_auto_backup_include_config")]
    pub auto_backup_include_config: bool,
    /// 自动备份保留天数
    #[serde(default = "default_auto_backup_retention_days")]
    pub auto_backup_retention_days: i32,
    #[serde(default = "default_auto_backup_retention_days_migrated")]
    pub auto_backup_retention_days_migrated: bool,
    /// 最近一次自动备份时间（ISO 8601）
    #[serde(default)]
    pub auto_backup_last_backup_at: Option<String>,
    #[serde(default = "default_webdav_sync_enabled")]
    pub webdav_sync_enabled: bool,
    #[serde(default = "default_webdav_sync_url")]
    pub webdav_sync_url: String,
    #[serde(default = "default_webdav_sync_username")]
    pub webdav_sync_username: String,
    #[serde(default = "default_webdav_sync_password")]
    pub webdav_sync_password: String,
    #[serde(default = "default_webdav_sync_remote_dir")]
    pub webdav_sync_remote_dir: String,
    #[serde(default = "default_webdav_sync_retention_days")]
    pub webdav_sync_retention_days: i32,
    #[serde(default)]
    pub webdav_sync_last_upload_at: Option<String>,
    #[serde(default)]
    pub webdav_sync_last_upload_file_name: Option<String>,
    #[serde(default)]
    pub webdav_sync_last_download_at: Option<String>,
    #[serde(default)]
    pub webdav_sync_last_download_file_name: Option<String>,
    /// 悬浮卡片保存的横向位置（物理像素）
    #[serde(default)]
    pub floating_card_position_x: Option<i32>,
    /// 悬浮卡片保存的纵向位置（物理像素）
    #[serde(default)]
    pub floating_card_position_y: Option<i32>,
    /// OpenCode 启动路径（为空则使用默认路径）
    #[serde(default = "default_opencode_app_path")]
    pub opencode_app_path: String,
    /// Antigravity IDE 启动路径（为空则使用默认路径）
    #[serde(default = "default_antigravity_app_path")]
    pub antigravity_app_path: String,
    /// Codex 启动路径（为空则使用默认路径）
    #[serde(default = "default_codex_app_path")]
    pub codex_app_path: String,
    #[serde(default = "default_codex_specified_app_path")]
    pub codex_specified_app_path: String,
    /// Zed 启动路径（为空则使用默认路径）
    #[serde(default = "default_zed_app_path")]
    pub zed_app_path: String,
    /// VS Code 启动路径（为空则使用默认路径）
    #[serde(default = "default_vscode_app_path")]
    pub vscode_app_path: String,
    /// Windsurf 启动路径（为空则使用默认路径）
    #[serde(default = "default_windsurf_app_path")]
    pub windsurf_app_path: String,
    /// Kiro 启动路径（为空则使用默认路径）
    #[serde(default = "default_kiro_app_path")]
    pub kiro_app_path: String,
    /// Cursor 启动路径（为空则使用默认路径）
    #[serde(default = "default_cursor_app_path")]
    pub cursor_app_path: String,
    /// CodeBuddy 启动路径（为空则使用默认路径）
    #[serde(default = "default_codebuddy_app_path")]
    pub codebuddy_app_path: String,
    /// CodeBuddy CN 启动路径（为空则使用默认路径）
    #[serde(default = "default_codebuddy_cn_app_path")]
    pub codebuddy_cn_app_path: String,
    /// Qoder 启动路径（为空则使用默认路径）
    #[serde(default = "default_qoder_app_path")]
    pub qoder_app_path: String,
    /// Trae 启动路径（为空则使用默认路径）
    #[serde(default = "default_trae_app_path")]
    pub trae_app_path: String,
    /// WorkBuddy 启动路径（为空则使用默认路径）
    #[serde(default = "default_workbuddy_app_path")]
    pub workbuddy_app_path: String,
    /// 切换 Codex 时是否自动重启 OpenCode
    #[serde(default = "default_opencode_sync_on_switch")]
    pub opencode_sync_on_switch: bool,
    /// 切换 Codex 时是否覆盖 OpenCode 登录信息
    #[serde(default = "default_opencode_auth_overwrite_on_switch")]
    pub opencode_auth_overwrite_on_switch: bool,
    /// 切换 GitHub Copilot 时是否自动重启 OpenCode
    #[serde(default = "default_ghcp_opencode_sync_on_switch")]
    pub ghcp_opencode_sync_on_switch: bool,
    /// 切换 GitHub Copilot 时是否覆盖 OpenCode 登录信息
    #[serde(default = "default_ghcp_opencode_auth_overwrite_on_switch")]
    pub ghcp_opencode_auth_overwrite_on_switch: bool,
    /// 切换 GitHub Copilot 时是否自动启动 GitHub Copilot
    #[serde(default = "default_ghcp_launch_on_switch")]
    pub ghcp_launch_on_switch: bool,
    /// 切换 Codex 时是否覆盖 OpenClaw 登录信息
    #[serde(default = "default_openclaw_auth_overwrite_on_switch")]
    pub openclaw_auth_overwrite_on_switch: bool,
    /// 切换 Codex 时是否自动启动/重启 Codex App
    #[serde(default = "default_codex_launch_on_switch")]
    pub codex_launch_on_switch: bool,
    #[serde(default = "default_codex_restart_specified_app_on_switch")]
    pub codex_restart_specified_app_on_switch: bool,
    #[serde(default = "default_codex_local_access_entry_visible")]
    pub codex_local_access_entry_visible: bool,
    #[serde(default = "default_top_right_ad_visible")]
    pub top_right_ad_visible: bool,
    /// Antigravity 切号是否启用“本地落盘 + 扩展无感”且不重启
    #[serde(default = "default_antigravity_dual_switch_no_restart_enabled")]
    pub antigravity_dual_switch_no_restart_enabled: bool,
    /// 是否启用自动切号
    #[serde(default = "default_auto_switch_enabled")]
    pub auto_switch_enabled: bool,
    /// 自动切号阈值（百分比），任意模型配额低于此值触发
    #[serde(default = "default_auto_switch_threshold")]
    pub auto_switch_threshold: i32,
    #[serde(default = "default_auto_switch_credits_enabled")]
    pub auto_switch_credits_enabled: bool,
    #[serde(default = "default_auto_switch_credits_threshold")]
    pub auto_switch_credits_threshold: i32,
    /// 自动切号触发模式：any_group | selected_groups
    #[serde(default = "default_auto_switch_scope_mode")]
    pub auto_switch_scope_mode: String,
    /// 自动切号指定模型分组（分组 ID）
    #[serde(default = "default_auto_switch_selected_group_ids")]
    pub auto_switch_selected_group_ids: Vec<String>,
    /// 自动切号账号范围模式：all_accounts | selected_accounts
    #[serde(default = "default_auto_switch_account_scope_mode")]
    pub auto_switch_account_scope_mode: String,
    /// 自动切号指定账号（账号 ID）
    #[serde(default = "default_auto_switch_selected_account_ids")]
    pub auto_switch_selected_account_ids: Vec<String>,
    /// 是否启用 Codex 自动切号
    #[serde(default = "default_codex_auto_switch_enabled")]
    pub codex_auto_switch_enabled: bool,
    /// Codex primary_window 自动切号阈值（百分比）
    #[serde(default = "default_codex_auto_switch_primary_threshold")]
    pub codex_auto_switch_primary_threshold: i32,
    /// Codex secondary_window 自动切号阈值（百分比）
    #[serde(default = "default_codex_auto_switch_secondary_threshold")]
    pub codex_auto_switch_secondary_threshold: i32,
    /// Codex 自动切号账号范围模式：all_accounts | selected_accounts
    #[serde(default = "default_codex_auto_switch_account_scope_mode")]
    pub codex_auto_switch_account_scope_mode: String,
    /// Codex 自动切号指定账号（账号 ID）
    #[serde(default = "default_codex_auto_switch_selected_account_ids")]
    pub codex_auto_switch_selected_account_ids: Vec<String>,
    /// 是否启用配额预警通知
    #[serde(default = "default_quota_alert_enabled")]
    pub quota_alert_enabled: bool,
    /// 配额预警阈值（百分比），任意模型配额低于此值触发
    #[serde(default = "default_quota_alert_threshold")]
    pub quota_alert_threshold: i32,
    /// 是否启用 Codex 配额预警通知
    #[serde(default = "default_codex_quota_alert_enabled")]
    pub codex_quota_alert_enabled: bool,
    /// Codex 配额预警阈值（百分比）
    #[serde(default = "default_codex_quota_alert_threshold")]
    pub codex_quota_alert_threshold: i32,
    /// 是否启用 Zed 配额预警通知
    #[serde(default = "default_zed_quota_alert_enabled")]
    pub zed_quota_alert_enabled: bool,
    /// Zed 配额预警阈值（百分比）
    #[serde(default = "default_zed_quota_alert_threshold")]
    pub zed_quota_alert_threshold: i32,
    /// Codex primary_window 配额预警阈值（百分比）
    #[serde(default = "default_codex_quota_alert_primary_threshold")]
    pub codex_quota_alert_primary_threshold: i32,
    /// Codex secondary_window 配额预警阈值（百分比）
    #[serde(default = "default_codex_quota_alert_secondary_threshold")]
    pub codex_quota_alert_secondary_threshold: i32,
    /// 是否启用 GitHub Copilot 配额预警通知
    #[serde(default = "default_ghcp_quota_alert_enabled")]
    pub ghcp_quota_alert_enabled: bool,
    /// GitHub Copilot 配额预警阈值（百分比）
    #[serde(default = "default_ghcp_quota_alert_threshold")]
    pub ghcp_quota_alert_threshold: i32,
    /// 是否启用 Windsurf 配额预警通知
    #[serde(default = "default_windsurf_quota_alert_enabled")]
    pub windsurf_quota_alert_enabled: bool,
    /// Windsurf 配额预警阈值（百分比）
    #[serde(default = "default_windsurf_quota_alert_threshold")]
    pub windsurf_quota_alert_threshold: i32,
    /// 是否启用 Kiro 配额预警通知
    #[serde(default = "default_kiro_quota_alert_enabled")]
    pub kiro_quota_alert_enabled: bool,
    /// Kiro 配额预警阈值（百分比）
    #[serde(default = "default_kiro_quota_alert_threshold")]
    pub kiro_quota_alert_threshold: i32,
    /// 是否启用 Cursor 配额预警通知
    #[serde(default = "default_cursor_quota_alert_enabled")]
    pub cursor_quota_alert_enabled: bool,
    /// Cursor 配额预警阈值（百分比）
    #[serde(default = "default_cursor_quota_alert_threshold")]
    pub cursor_quota_alert_threshold: i32,
    /// 是否启用 Gemini 配额预警通知
    #[serde(default = "default_gemini_quota_alert_enabled")]
    pub gemini_quota_alert_enabled: bool,
    /// Gemini 配额预警阈值（百分比）
    #[serde(default = "default_gemini_quota_alert_threshold")]
    pub gemini_quota_alert_threshold: i32,
    /// 是否启用 CodeBuddy 配额预警通知
    #[serde(default = "default_codebuddy_quota_alert_enabled")]
    pub codebuddy_quota_alert_enabled: bool,
    /// CodeBuddy 配额预警阈值（百分比）
    #[serde(default = "default_codebuddy_quota_alert_threshold")]
    pub codebuddy_quota_alert_threshold: i32,
    /// 是否启用 CodeBuddy CN 配额预警通知
    #[serde(default = "default_codebuddy_cn_quota_alert_enabled")]
    pub codebuddy_cn_quota_alert_enabled: bool,
    /// CodeBuddy CN 配额预警阈值（百分比）
    #[serde(default = "default_codebuddy_cn_quota_alert_threshold")]
    pub codebuddy_cn_quota_alert_threshold: i32,
    /// 是否启用 Qoder 配额预警通知
    #[serde(default = "default_qoder_quota_alert_enabled")]
    pub qoder_quota_alert_enabled: bool,
    /// Qoder 配额预警阈值（百分比）
    #[serde(default = "default_qoder_quota_alert_threshold")]
    pub qoder_quota_alert_threshold: i32,
    /// 是否启用 Trae 配额预警通知
    #[serde(default = "default_trae_quota_alert_enabled")]
    pub trae_quota_alert_enabled: bool,
    /// Trae 配额预警阈值（百分比）
    #[serde(default = "default_trae_quota_alert_threshold")]
    pub trae_quota_alert_threshold: i32,
    /// 是否启用 WorkBuddy 配额预警通知
    #[serde(default = "default_workbuddy_quota_alert_enabled")]
    pub workbuddy_quota_alert_enabled: bool,
    /// WorkBuddy 配额预警阈值（百分比）
    #[serde(default = "default_workbuddy_quota_alert_threshold")]
    pub workbuddy_quota_alert_threshold: i32,
}

/// 窗口关闭行为
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CloseWindowBehavior {
    /// 每次询问
    Ask,
    /// 最小化到托盘
    Minimize,
    /// 退出应用
    Quit,
}

impl Default for CloseWindowBehavior {
    fn default() -> Self {
        CloseWindowBehavior::Ask
    }
}

/// 窗口最小化行为（macOS）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MinimizeWindowBehavior {
    /// 程序坞 + 菜单栏（系统默认最小化）
    DockAndTray,
    /// 仅菜单栏（最小化时隐藏窗口）
    TrayOnly,
}

impl Default for MinimizeWindowBehavior {
    fn default() -> Self {
        MinimizeWindowBehavior::DockAndTray
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrayIconStyle {
    Template,
    Color,
}

impl TrayIconStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            TrayIconStyle::Template => "template",
            TrayIconStyle::Color => "color",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "color" => TrayIconStyle::Color,
            _ => TrayIconStyle::Template,
        }
    }
}

impl Default for TrayIconStyle {
    fn default() -> Self {
        TrayIconStyle::Template
    }
}

fn default_ws_enabled() -> bool {
    true
}
fn default_ws_port() -> u16 {
    DEFAULT_WS_PORT
}
fn default_report_enabled() -> bool {
    false
}
fn default_report_port() -> u16 {
    DEFAULT_REPORT_PORT
}
fn default_report_token() -> String {
    "change-this-token".to_string()
}
fn default_global_proxy_enabled() -> bool {
    false
}
fn default_global_proxy_url() -> String {
    String::new()
}
fn default_global_proxy_no_proxy() -> String {
    "127.0.0.1,localhost,::1".to_string()
}
fn default_language() -> String {
    "zh-cn".to_string()
}
fn default_default_terminal() -> String {
    "system".to_string()
}
fn default_theme() -> String {
    "system".to_string()
}
fn default_ui_scale() -> f64 {
    1.0
}
fn default_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_codex_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_codex_sync_wsl() -> bool {
    false
}
fn default_codex_wsl_config_dir() -> String {
    String::new()
}
fn default_zed_auto_refresh() -> i32 {
    10
}
fn default_ghcp_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_windsurf_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_kiro_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_cursor_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_gemini_auto_refresh() -> i32 {
    10
}
fn default_gemini_sync_wsl() -> bool {
    true
}
fn default_codebuddy_auto_refresh() -> i32 {
    10
}
fn default_codebuddy_cn_auto_refresh() -> i32 {
    10
}
fn default_workbuddy_auto_refresh() -> i32 {
    10
}
fn default_qoder_auto_refresh() -> i32 {
    10
}
fn default_trae_auto_refresh() -> i32 {
    10
}
fn default_close_behavior() -> CloseWindowBehavior {
    CloseWindowBehavior::Ask
}
fn default_minimize_behavior() -> MinimizeWindowBehavior {
    MinimizeWindowBehavior::DockAndTray
}
fn default_hide_dock_icon() -> bool {
    false
}
fn default_tray_icon_style() -> TrayIconStyle {
    TrayIconStyle::Template
}
fn default_floating_card_show_on_startup() -> bool {
    false
}
fn default_floating_card_always_on_top() -> bool {
    false
}
fn default_app_auto_launch_enabled() -> bool {
    false
}
fn default_antigravity_startup_wakeup_enabled() -> bool {
    false
}
fn default_antigravity_startup_wakeup_delay_seconds() -> i32 {
    0
}
fn default_codex_startup_wakeup_enabled() -> bool {
    false
}
fn default_codex_startup_wakeup_delay_seconds() -> i32 {
    0
}
fn default_floating_card_confirm_on_close() -> bool {
    true
}
pub fn default_auto_backup_enabled() -> bool {
    true
}
pub fn default_auto_backup_include_accounts() -> bool {
    true
}
pub fn default_auto_backup_include_config() -> bool {
    true
}
pub fn default_auto_backup_retention_days() -> i32 {
    15
}
pub fn default_auto_backup_retention_days_migrated() -> bool {
    true
}
pub fn sanitize_auto_backup_retention_days(raw: i32) -> i32 {
    raw.clamp(1, 365)
}
pub fn normalize_auto_backup_selection(
    include_accounts: bool,
    include_config: bool,
) -> (bool, bool) {
    if !include_accounts && !include_config {
        (
            default_auto_backup_include_accounts(),
            default_auto_backup_include_config(),
        )
    } else {
        (include_accounts, include_config)
    }
}

fn normalize_optional_config_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

pub fn default_webdav_sync_enabled() -> bool {
    true
}
pub fn default_webdav_sync_url() -> String {
    "https://dav.jianguoyun.com/dav/".to_string()
}
pub fn default_webdav_sync_username() -> String {
    String::new()
}
pub fn default_webdav_sync_password() -> String {
    String::new()
}
pub fn default_webdav_sync_remote_dir() -> String {
    "cockpit-tools".to_string()
}
pub fn default_webdav_sync_retention_days() -> i32 {
    15
}
pub fn sanitize_webdav_sync_retention_days(raw: i32) -> i32 {
    raw.clamp(1, 365)
}
fn default_opencode_app_path() -> String {
    String::new()
}
fn default_antigravity_app_path() -> String {
    String::new()
}
fn default_codex_app_path() -> String {
    String::new()
}
fn default_codex_specified_app_path() -> String {
    String::new()
}
fn default_zed_app_path() -> String {
    String::new()
}
fn default_vscode_app_path() -> String {
    String::new()
}
fn default_windsurf_app_path() -> String {
    String::new()
}
fn default_kiro_app_path() -> String {
    String::new()
}
fn default_cursor_app_path() -> String {
    String::new()
}
fn default_codebuddy_app_path() -> String {
    String::new()
}
fn default_codebuddy_cn_app_path() -> String {
    String::new()
}
fn default_qoder_app_path() -> String {
    String::new()
}
fn default_trae_app_path() -> String {
    String::new()
}
fn default_workbuddy_app_path() -> String {
    String::new()
}
fn default_opencode_sync_on_switch() -> bool {
    false
}
fn default_opencode_auth_overwrite_on_switch() -> bool {
    false
}
fn default_ghcp_opencode_sync_on_switch() -> bool {
    false
}
fn default_ghcp_opencode_auth_overwrite_on_switch() -> bool {
    false
}
fn default_ghcp_launch_on_switch() -> bool {
    true
}
fn default_openclaw_auth_overwrite_on_switch() -> bool {
    false
}
fn default_codex_launch_on_switch() -> bool {
    true
}
fn default_codex_restart_specified_app_on_switch() -> bool {
    false
}
fn default_codex_local_access_entry_visible() -> bool {
    true
}
fn default_top_right_ad_visible() -> bool {
    true
}
fn default_antigravity_dual_switch_no_restart_enabled() -> bool {
    false
}
fn default_auto_switch_enabled() -> bool {
    false
}
fn default_auto_switch_threshold() -> i32 {
    5
}
fn default_auto_switch_credits_enabled() -> bool {
    false
}
fn default_auto_switch_credits_threshold() -> i32 {
    5
}
fn default_auto_switch_scope_mode() -> String {
    "any_group".to_string()
}
fn default_auto_switch_selected_group_ids() -> Vec<String> {
    Vec::new()
}
fn default_auto_switch_account_scope_mode() -> String {
    "all_accounts".to_string()
}
fn default_auto_switch_selected_account_ids() -> Vec<String> {
    Vec::new()
}
fn default_codex_auto_switch_enabled() -> bool {
    false
}
fn default_codex_auto_switch_primary_threshold() -> i32 {
    20
}
fn default_codex_auto_switch_secondary_threshold() -> i32 {
    20
}
fn default_codex_auto_switch_account_scope_mode() -> String {
    "all_accounts".to_string()
}
fn default_codex_auto_switch_selected_account_ids() -> Vec<String> {
    Vec::new()
}
fn default_quota_alert_enabled() -> bool {
    false
}
fn default_quota_alert_threshold() -> i32 {
    20
}
fn default_codex_quota_alert_enabled() -> bool {
    false
}
fn default_codex_quota_alert_threshold() -> i32 {
    20
}
fn default_zed_quota_alert_enabled() -> bool {
    false
}
fn default_zed_quota_alert_threshold() -> i32 {
    20
}
fn default_codex_quota_alert_primary_threshold() -> i32 {
    20
}
fn default_codex_quota_alert_secondary_threshold() -> i32 {
    20
}
fn default_ghcp_quota_alert_enabled() -> bool {
    false
}
fn default_ghcp_quota_alert_threshold() -> i32 {
    20
}
fn default_windsurf_quota_alert_enabled() -> bool {
    false
}
fn default_windsurf_quota_alert_threshold() -> i32 {
    20
}
fn default_kiro_quota_alert_enabled() -> bool {
    false
}
fn default_kiro_quota_alert_threshold() -> i32 {
    20
}
fn default_cursor_quota_alert_enabled() -> bool {
    false
}
fn default_cursor_quota_alert_threshold() -> i32 {
    20
}
fn default_gemini_quota_alert_enabled() -> bool {
    false
}
fn default_gemini_quota_alert_threshold() -> i32 {
    20
}
fn default_codebuddy_quota_alert_enabled() -> bool {
    false
}
fn default_codebuddy_quota_alert_threshold() -> i32 {
    20
}
fn default_codebuddy_cn_quota_alert_enabled() -> bool {
    false
}
fn default_codebuddy_cn_quota_alert_threshold() -> i32 {
    20
}
fn default_qoder_quota_alert_enabled() -> bool {
    false
}
fn default_qoder_quota_alert_threshold() -> i32 {
    20
}
fn default_trae_quota_alert_enabled() -> bool {
    false
}
fn default_trae_quota_alert_threshold() -> i32 {
    20
}
fn default_workbuddy_quota_alert_enabled() -> bool {
    false
}
fn default_workbuddy_quota_alert_threshold() -> i32 {
    20
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            ws_enabled: true,
            ws_port: DEFAULT_WS_PORT,
            report_enabled: default_report_enabled(),
            report_port: default_report_port(),
            report_token: default_report_token(),
            global_proxy_enabled: default_global_proxy_enabled(),
            global_proxy_url: default_global_proxy_url(),
            global_proxy_no_proxy: default_global_proxy_no_proxy(),
            language: default_language(),
            default_terminal: default_default_terminal(),
            theme: default_theme(),
            ui_scale: default_ui_scale(),
            auto_refresh_minutes: default_auto_refresh(),
            codex_auto_refresh_minutes: default_codex_auto_refresh(),
            codex_sync_wsl: default_codex_sync_wsl(),
            codex_wsl_config_dir: default_codex_wsl_config_dir(),
            zed_auto_refresh_minutes: default_zed_auto_refresh(),
            ghcp_auto_refresh_minutes: default_ghcp_auto_refresh(),
            windsurf_auto_refresh_minutes: default_windsurf_auto_refresh(),
            kiro_auto_refresh_minutes: default_kiro_auto_refresh(),
            cursor_auto_refresh_minutes: default_cursor_auto_refresh(),
            gemini_auto_refresh_minutes: default_gemini_auto_refresh(),
            gemini_sync_wsl: default_gemini_sync_wsl(),
            codebuddy_auto_refresh_minutes: default_codebuddy_auto_refresh(),
            codebuddy_cn_auto_refresh_minutes: default_codebuddy_cn_auto_refresh(),
            workbuddy_auto_refresh_minutes: default_workbuddy_auto_refresh(),
            qoder_auto_refresh_minutes: default_qoder_auto_refresh(),
            trae_auto_refresh_minutes: default_trae_auto_refresh(),
            close_behavior: default_close_behavior(),
            minimize_behavior: default_minimize_behavior(),
            hide_dock_icon: default_hide_dock_icon(),
            tray_icon_style: default_tray_icon_style(),
            floating_card_show_on_startup: default_floating_card_show_on_startup(),
            floating_card_always_on_top: default_floating_card_always_on_top(),
            app_auto_launch_enabled: default_app_auto_launch_enabled(),
            antigravity_startup_wakeup_enabled: default_antigravity_startup_wakeup_enabled(),
            antigravity_startup_wakeup_delay_seconds:
                default_antigravity_startup_wakeup_delay_seconds(),
            codex_startup_wakeup_enabled: default_codex_startup_wakeup_enabled(),
            codex_startup_wakeup_delay_seconds: default_codex_startup_wakeup_delay_seconds(),
            floating_card_confirm_on_close: default_floating_card_confirm_on_close(),
            auto_backup_enabled: default_auto_backup_enabled(),
            auto_backup_include_accounts: default_auto_backup_include_accounts(),
            auto_backup_include_config: default_auto_backup_include_config(),
            auto_backup_retention_days: default_auto_backup_retention_days(),
            auto_backup_retention_days_migrated: default_auto_backup_retention_days_migrated(),
            auto_backup_last_backup_at: None,
            webdav_sync_enabled: default_webdav_sync_enabled(),
            webdav_sync_url: default_webdav_sync_url(),
            webdav_sync_username: default_webdav_sync_username(),
            webdav_sync_password: default_webdav_sync_password(),
            webdav_sync_remote_dir: default_webdav_sync_remote_dir(),
            webdav_sync_retention_days: default_webdav_sync_retention_days(),
            webdav_sync_last_upload_at: None,
            webdav_sync_last_upload_file_name: None,
            webdav_sync_last_download_at: None,
            webdav_sync_last_download_file_name: None,
            floating_card_position_x: None,
            floating_card_position_y: None,
            opencode_app_path: default_opencode_app_path(),
            antigravity_app_path: default_antigravity_app_path(),
            codex_app_path: default_codex_app_path(),
            codex_specified_app_path: default_codex_specified_app_path(),
            zed_app_path: default_zed_app_path(),
            vscode_app_path: default_vscode_app_path(),
            windsurf_app_path: default_windsurf_app_path(),
            kiro_app_path: default_kiro_app_path(),
            cursor_app_path: default_cursor_app_path(),
            codebuddy_app_path: default_codebuddy_app_path(),
            codebuddy_cn_app_path: default_codebuddy_cn_app_path(),
            qoder_app_path: default_qoder_app_path(),
            trae_app_path: default_trae_app_path(),
            workbuddy_app_path: default_workbuddy_app_path(),
            opencode_sync_on_switch: default_opencode_sync_on_switch(),
            opencode_auth_overwrite_on_switch: default_opencode_auth_overwrite_on_switch(),
            ghcp_opencode_sync_on_switch: default_ghcp_opencode_sync_on_switch(),
            ghcp_opencode_auth_overwrite_on_switch: default_ghcp_opencode_auth_overwrite_on_switch(
            ),
            ghcp_launch_on_switch: default_ghcp_launch_on_switch(),
            openclaw_auth_overwrite_on_switch: default_openclaw_auth_overwrite_on_switch(),
            codex_launch_on_switch: default_codex_launch_on_switch(),
            codex_restart_specified_app_on_switch: default_codex_restart_specified_app_on_switch(),
            codex_local_access_entry_visible: default_codex_local_access_entry_visible(),
            top_right_ad_visible: default_top_right_ad_visible(),
            antigravity_dual_switch_no_restart_enabled:
                default_antigravity_dual_switch_no_restart_enabled(),
            auto_switch_enabled: default_auto_switch_enabled(),
            auto_switch_threshold: default_auto_switch_threshold(),
            auto_switch_credits_enabled: default_auto_switch_credits_enabled(),
            auto_switch_credits_threshold: default_auto_switch_credits_threshold(),
            auto_switch_scope_mode: default_auto_switch_scope_mode(),
            auto_switch_selected_group_ids: default_auto_switch_selected_group_ids(),
            auto_switch_account_scope_mode: default_auto_switch_account_scope_mode(),
            auto_switch_selected_account_ids: default_auto_switch_selected_account_ids(),
            codex_auto_switch_enabled: default_codex_auto_switch_enabled(),
            codex_auto_switch_primary_threshold: default_codex_auto_switch_primary_threshold(),
            codex_auto_switch_secondary_threshold: default_codex_auto_switch_secondary_threshold(),
            codex_auto_switch_account_scope_mode: default_codex_auto_switch_account_scope_mode(),
            codex_auto_switch_selected_account_ids: default_codex_auto_switch_selected_account_ids(
            ),
            quota_alert_enabled: default_quota_alert_enabled(),
            quota_alert_threshold: default_quota_alert_threshold(),
            codex_quota_alert_enabled: default_codex_quota_alert_enabled(),
            codex_quota_alert_threshold: default_codex_quota_alert_threshold(),
            zed_quota_alert_enabled: default_zed_quota_alert_enabled(),
            zed_quota_alert_threshold: default_zed_quota_alert_threshold(),
            codex_quota_alert_primary_threshold: default_codex_quota_alert_primary_threshold(),
            codex_quota_alert_secondary_threshold: default_codex_quota_alert_secondary_threshold(),
            ghcp_quota_alert_enabled: default_ghcp_quota_alert_enabled(),
            ghcp_quota_alert_threshold: default_ghcp_quota_alert_threshold(),
            windsurf_quota_alert_enabled: default_windsurf_quota_alert_enabled(),
            windsurf_quota_alert_threshold: default_windsurf_quota_alert_threshold(),
            kiro_quota_alert_enabled: default_kiro_quota_alert_enabled(),
            kiro_quota_alert_threshold: default_kiro_quota_alert_threshold(),
            cursor_quota_alert_enabled: default_cursor_quota_alert_enabled(),
            cursor_quota_alert_threshold: default_cursor_quota_alert_threshold(),
            gemini_quota_alert_enabled: default_gemini_quota_alert_enabled(),
            gemini_quota_alert_threshold: default_gemini_quota_alert_threshold(),
            codebuddy_quota_alert_enabled: default_codebuddy_quota_alert_enabled(),
            codebuddy_quota_alert_threshold: default_codebuddy_quota_alert_threshold(),
            codebuddy_cn_quota_alert_enabled: default_codebuddy_cn_quota_alert_enabled(),
            codebuddy_cn_quota_alert_threshold: default_codebuddy_cn_quota_alert_threshold(),
            qoder_quota_alert_enabled: default_qoder_quota_alert_enabled(),
            qoder_quota_alert_threshold: default_qoder_quota_alert_threshold(),
            trae_quota_alert_enabled: default_trae_quota_alert_enabled(),
            trae_quota_alert_threshold: default_trae_quota_alert_threshold(),
            workbuddy_quota_alert_enabled: default_workbuddy_quota_alert_enabled(),
            workbuddy_quota_alert_threshold: default_workbuddy_quota_alert_threshold(),
        }
    }
}

/// 运行时状态
struct RuntimeState {
    /// 当前实际使用的端口
    actual_port: Option<u16>,
    /// 用户配置
    user_config: UserConfig,
}

/// 全局运行时状态
static RUNTIME_STATE: OnceLock<RwLock<RuntimeState>> = OnceLock::new();
static INHERITED_PROXY_ENV: OnceLock<Vec<(&'static str, Option<String>)>> = OnceLock::new();

fn get_runtime_state() -> &'static RwLock<RuntimeState> {
    RUNTIME_STATE.get_or_init(|| {
        let initial_config = load_user_config().unwrap_or_default();
        // 让应用内 reqwest 客户端与用户全局代理设置保持一致。
        sync_global_proxy_env(&initial_config);
        RwLock::new(RuntimeState {
            actual_port: None,
            user_config: initial_config,
        })
    })
}

const MANAGED_PROXY_SET_KEYS: [&str; 6] = [
    "http_proxy",
    "https_proxy",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "all_proxy",
    "ALL_PROXY",
];

const MANAGED_PROXY_NO_PROXY_KEYS: [&str; 2] = ["no_proxy", "NO_PROXY"];

fn inherited_proxy_env() -> &'static Vec<(&'static str, Option<String>)> {
    INHERITED_PROXY_ENV.get_or_init(|| {
        MANAGED_PROXY_SET_KEYS
            .iter()
            .chain(MANAGED_PROXY_NO_PROXY_KEYS.iter())
            .map(|key| (*key, std::env::var(key).ok()))
            .collect()
    })
}

fn managed_proxy_env_pairs(config: &UserConfig) -> Vec<(&'static str, String)> {
    if !config.global_proxy_enabled {
        return Vec::new();
    }

    let proxy_url = config.global_proxy_url.trim();
    if proxy_url.is_empty() {
        return Vec::new();
    }

    let mut pairs = Vec::with_capacity(8);
    for key in MANAGED_PROXY_SET_KEYS {
        pairs.push((key, proxy_url.to_string()));
    }

    let no_proxy = config.global_proxy_no_proxy.trim();
    if !no_proxy.is_empty() {
        for key in MANAGED_PROXY_NO_PROXY_KEYS {
            pairs.push((key, no_proxy.to_string()));
        }
    }

    pairs
}

fn clear_managed_proxy_env() {
    for key in MANAGED_PROXY_SET_KEYS {
        std::env::remove_var(key);
    }
    for key in MANAGED_PROXY_NO_PROXY_KEYS {
        std::env::remove_var(key);
    }
}

fn restore_inherited_proxy_env() {
    clear_managed_proxy_env();

    let mut restored_keys = Vec::new();
    for (key, value) in inherited_proxy_env() {
        if let Some(value) = value {
            std::env::set_var(key, value);
            restored_keys.push(*key);
        }
    }

    if restored_keys.is_empty() {
        crate::modules::logger::log_info(
            "[Proxy] 应用内未启用全局代理，已恢复启动时继承环境（未携带代理变量）",
        );
        return;
    }

    crate::modules::logger::log_info(&format!(
        "[Proxy] 应用内未启用全局代理，已恢复启动时继承环境 keys={}",
        restored_keys.join(",")
    ));
}

pub fn sync_global_proxy_env(config: &UserConfig) {
    let pairs = managed_proxy_env_pairs(config);
    if pairs.is_empty() {
        restore_inherited_proxy_env();
        return;
    }

    clear_managed_proxy_env();

    let mut applied_keys = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        std::env::set_var(key, value);
        applied_keys.push(key);
    }

    crate::modules::logger::log_info(&format!(
        "[Proxy] 应用内全局代理环境已同步 keys={}",
        applied_keys.join(",")
    ));
}

/// 获取数据目录路径
pub fn get_data_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法获取 Home 目录")?;
    Ok(home.join(DATA_DIR))
}

/// 获取共享目录路径（供其他模块使用）
/// 与 get_data_dir 相同，但不返回 Result
pub fn get_shared_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(DATA_DIR))
        .unwrap_or_else(|| PathBuf::from(DATA_DIR))
}

/// 获取服务状态文件路径
pub fn get_server_status_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join(SERVER_STATUS_FILE))
}

/// 获取用户配置文件路径
pub fn get_user_config_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join(USER_CONFIG_FILE))
}

/// 加载用户配置
pub fn load_user_config() -> Result<UserConfig, String> {
    let config_path = get_user_config_path()?;

    if !config_path.exists() {
        return Ok(UserConfig::default());
    }

    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("读取配置文件失败: {}", e))?;

    let mut value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(error) => {
            match crate::modules::atomic_write::quarantine_file(&config_path, "invalid-json") {
                Ok(Some(backup_path)) => crate::modules::logger::log_warn(&format!(
                    "配置文件解析失败，已隔离并使用默认配置: path={}, backup={}, error={}",
                    config_path.display(),
                    backup_path.display(),
                    error
                )),
                Ok(None) => crate::modules::logger::log_warn(&format!(
                    "配置文件解析失败，文件已不存在，使用默认配置: path={}, error={}",
                    config_path.display(),
                    error
                )),
                Err(backup_error) => crate::modules::logger::log_warn(&format!(
                    "配置文件解析失败，隔离失败，使用默认配置: path={}, parse_error={}, backup_error={}",
                    config_path.display(),
                    error,
                    backup_error
                )),
            }
            return Ok(UserConfig::default());
        }
    };

    // 兼容旧配置：平台独立预警字段不存在时，继承历史全局预警配置
    if let Some(obj) = value.as_object_mut() {
        if !obj.contains_key("kiro_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("windsurf_auto_refresh_minutes")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_kiro_auto_refresh);
            obj.insert(
                "kiro_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("cursor_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("kiro_auto_refresh_minutes")
                .or_else(|| obj.get("windsurf_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_cursor_auto_refresh);
            obj.insert(
                "cursor_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("gemini_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("cursor_auto_refresh_minutes")
                .or_else(|| obj.get("kiro_auto_refresh_minutes"))
                .or_else(|| obj.get("windsurf_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_gemini_auto_refresh);
            obj.insert(
                "gemini_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("codex_sync_wsl") {
            obj.insert(
                "codex_sync_wsl".to_string(),
                json!(default_codex_sync_wsl()),
            );
        }

        if !obj.contains_key("codex_wsl_config_dir") {
            obj.insert(
                "codex_wsl_config_dir".to_string(),
                json!(default_codex_wsl_config_dir()),
            );
        }

        if !obj.contains_key("gemini_sync_wsl") {
            obj.insert(
                "gemini_sync_wsl".to_string(),
                json!(default_gemini_sync_wsl()),
            );
        }

        if !obj.contains_key("qoder_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("gemini_auto_refresh_minutes")
                .or_else(|| obj.get("cursor_auto_refresh_minutes"))
                .or_else(|| obj.get("kiro_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_qoder_auto_refresh);
            obj.insert(
                "qoder_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("codebuddy_cn_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("codebuddy_auto_refresh_minutes")
                .or_else(|| obj.get("gemini_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_codebuddy_cn_auto_refresh);
            obj.insert(
                "codebuddy_cn_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("workbuddy_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("codebuddy_cn_auto_refresh_minutes")
                .or_else(|| obj.get("codebuddy_auto_refresh_minutes"))
                .or_else(|| obj.get("gemini_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_workbuddy_auto_refresh);
            obj.insert(
                "workbuddy_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("trae_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("qoder_auto_refresh_minutes")
                .or_else(|| obj.get("gemini_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_trae_auto_refresh);
            obj.insert(
                "trae_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("hide_dock_icon") {
            let inherited_hide_dock_icon = obj
                .get("minimize_behavior")
                .and_then(|v| v.as_str())
                .map(|v| v == "tray_only")
                .unwrap_or_else(default_hide_dock_icon);
            obj.insert(
                "hide_dock_icon".to_string(),
                json!(inherited_hide_dock_icon),
            );
        }

        if !obj.contains_key("tray_icon_style") {
            obj.insert(
                "tray_icon_style".to_string(),
                json!(default_tray_icon_style()),
            );
        }

        if !obj.contains_key("floating_card_show_on_startup") {
            obj.insert(
                "floating_card_show_on_startup".to_string(),
                json!(default_floating_card_show_on_startup()),
            );
        }

        if !obj.contains_key("floating_card_always_on_top") {
            obj.insert(
                "floating_card_always_on_top".to_string(),
                json!(default_floating_card_always_on_top()),
            );
        }

        if !obj.contains_key("app_auto_launch_enabled") {
            obj.insert(
                "app_auto_launch_enabled".to_string(),
                json!(default_app_auto_launch_enabled()),
            );
        }

        if !obj.contains_key("antigravity_startup_wakeup_enabled") {
            obj.insert(
                "antigravity_startup_wakeup_enabled".to_string(),
                json!(default_antigravity_startup_wakeup_enabled()),
            );
        }

        if !obj.contains_key("antigravity_startup_wakeup_delay_seconds") {
            obj.insert(
                "antigravity_startup_wakeup_delay_seconds".to_string(),
                json!(default_antigravity_startup_wakeup_delay_seconds()),
            );
        }

        if !obj.contains_key("codex_startup_wakeup_enabled") {
            obj.insert(
                "codex_startup_wakeup_enabled".to_string(),
                json!(default_codex_startup_wakeup_enabled()),
            );
        }

        if !obj.contains_key("codex_startup_wakeup_delay_seconds") {
            obj.insert(
                "codex_startup_wakeup_delay_seconds".to_string(),
                json!(default_codex_startup_wakeup_delay_seconds()),
            );
        }

        if !obj.contains_key("codex_local_access_entry_visible") {
            obj.insert(
                "codex_local_access_entry_visible".to_string(),
                json!(default_codex_local_access_entry_visible()),
            );
        }

        if !obj.contains_key("top_right_ad_visible") {
            obj.insert(
                "top_right_ad_visible".to_string(),
                json!(default_top_right_ad_visible()),
            );
        }

        if !obj.contains_key("floating_card_confirm_on_close") {
            obj.insert(
                "floating_card_confirm_on_close".to_string(),
                json!(default_floating_card_confirm_on_close()),
            );
        }
        if !obj.contains_key("auto_backup_enabled") {
            obj.insert(
                "auto_backup_enabled".to_string(),
                json!(default_auto_backup_enabled()),
            );
        }
        if !obj.contains_key("auto_backup_include_accounts") {
            obj.insert(
                "auto_backup_include_accounts".to_string(),
                json!(default_auto_backup_include_accounts()),
            );
        }
        if !obj.contains_key("auto_backup_include_config") {
            obj.insert(
                "auto_backup_include_config".to_string(),
                json!(default_auto_backup_include_config()),
            );
        }
        if !obj.contains_key("auto_backup_retention_days") {
            obj.insert(
                "auto_backup_retention_days".to_string(),
                json!(default_auto_backup_retention_days()),
            );
        }
        if !obj.contains_key("auto_backup_retention_days_migrated") {
            obj.insert(
                "auto_backup_retention_days_migrated".to_string(),
                json!(default_auto_backup_retention_days_migrated()),
            );
        }
        if !obj.contains_key("auto_backup_last_backup_at") {
            obj.insert(
                "auto_backup_last_backup_at".to_string(),
                serde_json::Value::Null,
            );
        }
        if !obj.contains_key("webdav_sync_enabled") {
            obj.insert(
                "webdav_sync_enabled".to_string(),
                json!(default_webdav_sync_enabled()),
            );
        }
        if !obj.contains_key("webdav_sync_url") {
            obj.insert(
                "webdav_sync_url".to_string(),
                json!(default_webdav_sync_url()),
            );
        }
        if !obj.contains_key("webdav_sync_username") {
            obj.insert(
                "webdav_sync_username".to_string(),
                json!(default_webdav_sync_username()),
            );
        }
        if !obj.contains_key("webdav_sync_password") {
            obj.insert(
                "webdav_sync_password".to_string(),
                json!(default_webdav_sync_password()),
            );
        }
        if !obj.contains_key("webdav_sync_remote_dir") {
            obj.insert(
                "webdav_sync_remote_dir".to_string(),
                json!(default_webdav_sync_remote_dir()),
            );
        }
        if !obj.contains_key("webdav_sync_retention_days") {
            obj.insert(
                "webdav_sync_retention_days".to_string(),
                json!(default_webdav_sync_retention_days()),
            );
        }
        if !obj.contains_key("webdav_sync_last_upload_at") {
            obj.insert(
                "webdav_sync_last_upload_at".to_string(),
                serde_json::Value::Null,
            );
        }
        if !obj.contains_key("webdav_sync_last_upload_file_name") {
            obj.insert(
                "webdav_sync_last_upload_file_name".to_string(),
                serde_json::Value::Null,
            );
        }
        if !obj.contains_key("webdav_sync_last_download_at") {
            obj.insert(
                "webdav_sync_last_download_at".to_string(),
                serde_json::Value::Null,
            );
        }
        if !obj.contains_key("webdav_sync_last_download_file_name") {
            obj.insert(
                "webdav_sync_last_download_file_name".to_string(),
                serde_json::Value::Null,
            );
        }

        if !obj.contains_key("report_enabled") {
            obj.insert(
                "report_enabled".to_string(),
                json!(default_report_enabled()),
            );
        }
        if !obj.contains_key("report_port") {
            obj.insert("report_port".to_string(), json!(default_report_port()));
        }
        if !obj.contains_key("report_token") {
            obj.insert("report_token".to_string(), json!(default_report_token()));
        }
        if !obj.contains_key("default_terminal") {
            obj.insert(
                "default_terminal".to_string(),
                json!(default_default_terminal()),
            );
        }
        if !obj.contains_key("global_proxy_enabled") {
            obj.insert(
                "global_proxy_enabled".to_string(),
                json!(default_global_proxy_enabled()),
            );
        }
        if !obj.contains_key("global_proxy_url") {
            obj.insert(
                "global_proxy_url".to_string(),
                json!(default_global_proxy_url()),
            );
        }
        if !obj.contains_key("global_proxy_no_proxy") {
            obj.insert(
                "global_proxy_no_proxy".to_string(),
                json!(default_global_proxy_no_proxy()),
            );
        }

        let legacy_enabled = obj
            .get("quota_alert_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(default_quota_alert_enabled);
        let legacy_threshold = obj
            .get("quota_alert_threshold")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or_else(default_quota_alert_threshold);
        let legacy_auto_switch_enabled = obj
            .get("codex_auto_switch_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(default_codex_auto_switch_enabled);
        let legacy_auto_switch_threshold = obj
            .get("codex_auto_switch_primary_threshold")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or_else(default_codex_auto_switch_primary_threshold);

        if !obj.contains_key("codex_quota_alert_enabled") {
            obj.insert(
                "codex_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("codex_quota_alert_threshold") {
            obj.insert(
                "codex_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("zed_quota_alert_enabled") {
            obj.insert("zed_quota_alert_enabled".to_string(), json!(legacy_enabled));
        }
        if !obj.contains_key("zed_quota_alert_threshold") {
            obj.insert(
                "zed_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("codex_auto_switch_enabled") {
            obj.insert(
                "codex_auto_switch_enabled".to_string(),
                json!(legacy_auto_switch_enabled),
            );
        }
        if !obj.contains_key("codex_auto_switch_primary_threshold") {
            obj.insert(
                "codex_auto_switch_primary_threshold".to_string(),
                json!(legacy_auto_switch_threshold),
            );
        }
        if !obj.contains_key("codex_auto_switch_secondary_threshold") {
            obj.insert(
                "codex_auto_switch_secondary_threshold".to_string(),
                json!(legacy_auto_switch_threshold),
            );
        }
        if !obj.contains_key("auto_switch_scope_mode") {
            obj.insert(
                "auto_switch_scope_mode".to_string(),
                json!(default_auto_switch_scope_mode()),
            );
        }
        if !obj.contains_key("auto_switch_credits_enabled") {
            obj.insert(
                "auto_switch_credits_enabled".to_string(),
                json!(default_auto_switch_credits_enabled()),
            );
        }
        if !obj.contains_key("auto_switch_credits_threshold") {
            obj.insert(
                "auto_switch_credits_threshold".to_string(),
                json!(default_auto_switch_credits_threshold()),
            );
        }
        if !obj.contains_key("auto_switch_selected_group_ids") {
            obj.insert(
                "auto_switch_selected_group_ids".to_string(),
                json!(default_auto_switch_selected_group_ids()),
            );
        }
        if !obj.contains_key("auto_switch_account_scope_mode") {
            obj.insert(
                "auto_switch_account_scope_mode".to_string(),
                json!(default_auto_switch_account_scope_mode()),
            );
        }
        if !obj.contains_key("auto_switch_selected_account_ids") {
            obj.insert(
                "auto_switch_selected_account_ids".to_string(),
                json!(default_auto_switch_selected_account_ids()),
            );
        }
        if !obj.contains_key("codex_auto_switch_account_scope_mode") {
            obj.insert(
                "codex_auto_switch_account_scope_mode".to_string(),
                json!(default_codex_auto_switch_account_scope_mode()),
            );
        }
        if !obj.contains_key("codex_auto_switch_selected_account_ids") {
            obj.insert(
                "codex_auto_switch_selected_account_ids".to_string(),
                json!(default_codex_auto_switch_selected_account_ids()),
            );
        }
        let codex_legacy_threshold = obj
            .get("codex_quota_alert_threshold")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(legacy_threshold);
        if !obj.contains_key("codex_quota_alert_primary_threshold") {
            obj.insert(
                "codex_quota_alert_primary_threshold".to_string(),
                json!(codex_legacy_threshold),
            );
        }
        if !obj.contains_key("codex_quota_alert_secondary_threshold") {
            obj.insert(
                "codex_quota_alert_secondary_threshold".to_string(),
                json!(codex_legacy_threshold),
            );
        }
        if !obj.contains_key("ghcp_quota_alert_enabled") {
            obj.insert(
                "ghcp_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("ghcp_quota_alert_threshold") {
            obj.insert(
                "ghcp_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("windsurf_quota_alert_enabled") {
            obj.insert(
                "windsurf_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("windsurf_quota_alert_threshold") {
            obj.insert(
                "windsurf_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("kiro_quota_alert_enabled") {
            obj.insert(
                "kiro_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("kiro_quota_alert_threshold") {
            obj.insert(
                "kiro_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("cursor_quota_alert_enabled") {
            obj.insert(
                "cursor_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("cursor_quota_alert_threshold") {
            obj.insert(
                "cursor_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("gemini_quota_alert_enabled") {
            obj.insert(
                "gemini_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("gemini_quota_alert_threshold") {
            obj.insert(
                "gemini_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("codebuddy_quota_alert_enabled") {
            obj.insert(
                "codebuddy_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("codebuddy_quota_alert_threshold") {
            obj.insert(
                "codebuddy_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("codebuddy_cn_quota_alert_enabled") {
            obj.insert(
                "codebuddy_cn_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("codebuddy_cn_quota_alert_threshold") {
            obj.insert(
                "codebuddy_cn_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("qoder_quota_alert_enabled") {
            obj.insert(
                "qoder_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("qoder_quota_alert_threshold") {
            obj.insert(
                "qoder_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("trae_quota_alert_enabled") {
            obj.insert(
                "trae_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("trae_quota_alert_threshold") {
            obj.insert(
                "trae_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("workbuddy_quota_alert_enabled") {
            obj.insert(
                "workbuddy_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("workbuddy_quota_alert_threshold") {
            obj.insert(
                "workbuddy_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
    }

    let mut config: UserConfig = match serde_json::from_value(value) {
        Ok(config) => config,
        Err(error) => {
            match crate::modules::atomic_write::quarantine_file(&config_path, "invalid-shape") {
                Ok(Some(backup_path)) => crate::modules::logger::log_warn(&format!(
                    "配置文件结构无效，已隔离并使用默认配置: path={}, backup={}, error={}",
                    config_path.display(),
                    backup_path.display(),
                    error
                )),
                Ok(None) => crate::modules::logger::log_warn(&format!(
                    "配置文件结构无效，文件已不存在，使用默认配置: path={}, error={}",
                    config_path.display(),
                    error
                )),
                Err(backup_error) => crate::modules::logger::log_warn(&format!(
                    "配置文件结构无效，隔离失败，使用默认配置: path={}, parse_error={}, backup_error={}",
                    config_path.display(),
                    error,
                    backup_error
                )),
            }
            return Ok(UserConfig::default());
        }
    };
    let (include_accounts, include_config) = normalize_auto_backup_selection(
        config.auto_backup_include_accounts,
        config.auto_backup_include_config,
    );
    config.auto_backup_include_accounts = include_accounts;
    config.auto_backup_include_config = include_config;
    config.auto_backup_retention_days =
        sanitize_auto_backup_retention_days(config.auto_backup_retention_days);
    config.auto_backup_last_backup_at = config.auto_backup_last_backup_at.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    config.webdav_sync_retention_days =
        sanitize_webdav_sync_retention_days(config.webdav_sync_retention_days);
    config.webdav_sync_last_upload_at =
        normalize_optional_config_string(config.webdav_sync_last_upload_at);
    config.webdav_sync_last_upload_file_name =
        normalize_optional_config_string(config.webdav_sync_last_upload_file_name);
    config.webdav_sync_last_download_at =
        normalize_optional_config_string(config.webdav_sync_last_download_at);
    config.webdav_sync_last_download_file_name =
        normalize_optional_config_string(config.webdav_sync_last_download_file_name);

    Ok(config)
}

/// 保存用户配置
pub fn save_user_config(config: &UserConfig) -> Result<(), String> {
    let config_path = get_user_config_path()?;
    let data_dir = get_data_dir()?;

    // 确保目录存在
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }

    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {}", e))?;

    crate::modules::atomic_write::write_string_atomic(&config_path, &json)
        .map_err(|e| format!("写入配置文件失败: {}", e))?;

    // 更新运行时状态
    if let Ok(mut state) = get_runtime_state().write() {
        state.user_config = config.clone();
    }

    sync_global_proxy_env(config);

    crate::modules::logger::log_info(&format!(
        "[Config] 用户配置已保存: ws_enabled={}, ws_port={}, report_enabled={}, report_port={}",
        config.ws_enabled, config.ws_port, config.report_enabled, config.report_port
    ));

    Ok(())
}

/// 获取用户配置（从内存）
pub fn get_user_config() -> UserConfig {
    get_runtime_state()
        .read()
        .map(|state| state.user_config.clone())
        .unwrap_or_default()
}

/// 获取用户配置的首选端口
pub fn get_preferred_port() -> u16 {
    get_user_config().ws_port
}

/// 获取当前实际使用的端口
pub fn get_actual_port() -> Option<u16> {
    get_runtime_state()
        .read()
        .ok()
        .and_then(|state| state.actual_port)
}

const MAX_STARTUP_WAKEUP_DELAY_SECONDS: i32 = 24 * 60 * 60;
const DEFAULT_UI_SCALE: f64 = 1.0;
const MIN_UI_SCALE: f64 = 0.8;
const MAX_UI_SCALE: f64 = 2.0;
const AUTO_SWITCH_ACCOUNT_SCOPE_ALL: &str = "all_accounts";
const AUTO_SWITCH_ACCOUNT_SCOPE_SELECTED: &str = "selected_accounts";

pub fn sanitize_ui_scale(raw: f64) -> f64 {
    if !raw.is_finite() {
        return DEFAULT_UI_SCALE;
    }
    raw.clamp(MIN_UI_SCALE, MAX_UI_SCALE)
}

pub fn sanitize_startup_wakeup_delay_seconds(raw: i32) -> i32 {
    raw.clamp(0, MAX_STARTUP_WAKEUP_DELAY_SECONDS)
}

pub fn normalize_auto_switch_account_scope_mode(raw: &str) -> String {
    if raw.trim().to_lowercase() == AUTO_SWITCH_ACCOUNT_SCOPE_SELECTED {
        AUTO_SWITCH_ACCOUNT_SCOPE_SELECTED.to_string()
    } else {
        AUTO_SWITCH_ACCOUNT_SCOPE_ALL.to_string()
    }
}

pub fn normalize_auto_switch_selected_account_ids(raw: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in raw {
        let normalized = item.trim().to_string();
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        result.push(normalized);
    }
    result
}

pub fn general_config_from_user_config(
    user_config: UserConfig,
    app_auto_launch_enabled_override: Option<bool>,
) -> GeneralConfig {
    let app_auto_launch_enabled =
        app_auto_launch_enabled_override.unwrap_or(user_config.app_auto_launch_enabled);
    let close_behavior = match user_config.close_behavior {
        CloseWindowBehavior::Ask => "ask",
        CloseWindowBehavior::Minimize => "minimize",
        CloseWindowBehavior::Quit => "quit",
    };
    let minimize_behavior = match user_config.minimize_behavior {
        MinimizeWindowBehavior::DockAndTray => "dock_and_tray",
        MinimizeWindowBehavior::TrayOnly => "tray_only",
    };

    GeneralConfig {
        language: user_config.language,
        default_terminal: user_config.default_terminal,
        theme: user_config.theme,
        ui_scale: user_config.ui_scale,
        auto_refresh_minutes: user_config.auto_refresh_minutes,
        codex_auto_refresh_minutes: user_config.codex_auto_refresh_minutes,
        codex_sync_wsl: user_config.codex_sync_wsl,
        codex_wsl_config_dir: user_config.codex_wsl_config_dir,
        zed_auto_refresh_minutes: user_config.zed_auto_refresh_minutes,
        ghcp_auto_refresh_minutes: user_config.ghcp_auto_refresh_minutes,
        windsurf_auto_refresh_minutes: user_config.windsurf_auto_refresh_minutes,
        kiro_auto_refresh_minutes: user_config.kiro_auto_refresh_minutes,
        cursor_auto_refresh_minutes: user_config.cursor_auto_refresh_minutes,
        gemini_auto_refresh_minutes: user_config.gemini_auto_refresh_minutes,
        gemini_sync_wsl: user_config.gemini_sync_wsl,
        codebuddy_auto_refresh_minutes: user_config.codebuddy_auto_refresh_minutes,
        codebuddy_cn_auto_refresh_minutes: user_config.codebuddy_cn_auto_refresh_minutes,
        workbuddy_auto_refresh_minutes: user_config.workbuddy_auto_refresh_minutes,
        qoder_auto_refresh_minutes: user_config.qoder_auto_refresh_minutes,
        trae_auto_refresh_minutes: user_config.trae_auto_refresh_minutes,
        close_behavior: close_behavior.to_string(),
        minimize_behavior: minimize_behavior.to_string(),
        hide_dock_icon: user_config.hide_dock_icon,
        tray_icon_style: user_config.tray_icon_style.as_str().to_string(),
        floating_card_show_on_startup: user_config.floating_card_show_on_startup,
        floating_card_always_on_top: user_config.floating_card_always_on_top,
        app_auto_launch_enabled,
        antigravity_startup_wakeup_enabled: user_config.antigravity_startup_wakeup_enabled,
        antigravity_startup_wakeup_delay_seconds: sanitize_startup_wakeup_delay_seconds(
            user_config.antigravity_startup_wakeup_delay_seconds,
        ),
        codex_startup_wakeup_enabled: user_config.codex_startup_wakeup_enabled,
        codex_startup_wakeup_delay_seconds: sanitize_startup_wakeup_delay_seconds(
            user_config.codex_startup_wakeup_delay_seconds,
        ),
        floating_card_confirm_on_close: user_config.floating_card_confirm_on_close,
        opencode_app_path: user_config.opencode_app_path,
        antigravity_app_path: user_config.antigravity_app_path,
        codex_app_path: user_config.codex_app_path,
        codex_specified_app_path: user_config.codex_specified_app_path,
        zed_app_path: user_config.zed_app_path,
        vscode_app_path: user_config.vscode_app_path,
        windsurf_app_path: user_config.windsurf_app_path,
        kiro_app_path: user_config.kiro_app_path,
        cursor_app_path: user_config.cursor_app_path,
        codebuddy_app_path: user_config.codebuddy_app_path,
        codebuddy_cn_app_path: user_config.codebuddy_cn_app_path,
        qoder_app_path: user_config.qoder_app_path,
        trae_app_path: user_config.trae_app_path,
        workbuddy_app_path: user_config.workbuddy_app_path,
        opencode_sync_on_switch: user_config.opencode_sync_on_switch,
        opencode_auth_overwrite_on_switch: user_config.opencode_auth_overwrite_on_switch,
        ghcp_opencode_sync_on_switch: user_config.ghcp_opencode_sync_on_switch,
        ghcp_opencode_auth_overwrite_on_switch: user_config.ghcp_opencode_auth_overwrite_on_switch,
        ghcp_launch_on_switch: user_config.ghcp_launch_on_switch,
        openclaw_auth_overwrite_on_switch: user_config.openclaw_auth_overwrite_on_switch,
        codex_launch_on_switch: user_config.codex_launch_on_switch,
        codex_restart_specified_app_on_switch: user_config.codex_restart_specified_app_on_switch,
        codex_local_access_entry_visible: user_config.codex_local_access_entry_visible,
        top_right_ad_visible: user_config.top_right_ad_visible,
        antigravity_dual_switch_no_restart_enabled: user_config
            .antigravity_dual_switch_no_restart_enabled,
        auto_switch_enabled: user_config.auto_switch_enabled,
        auto_switch_threshold: user_config.auto_switch_threshold,
        auto_switch_credits_enabled: user_config.auto_switch_credits_enabled,
        auto_switch_credits_threshold: user_config.auto_switch_credits_threshold,
        auto_switch_scope_mode: user_config.auto_switch_scope_mode,
        auto_switch_selected_group_ids: user_config.auto_switch_selected_group_ids,
        auto_switch_account_scope_mode: user_config.auto_switch_account_scope_mode,
        auto_switch_selected_account_ids: user_config.auto_switch_selected_account_ids,
        codex_auto_switch_enabled: user_config.codex_auto_switch_enabled,
        codex_auto_switch_primary_threshold: user_config.codex_auto_switch_primary_threshold,
        codex_auto_switch_secondary_threshold: user_config.codex_auto_switch_secondary_threshold,
        codex_auto_switch_account_scope_mode: user_config.codex_auto_switch_account_scope_mode,
        codex_auto_switch_selected_account_ids: user_config.codex_auto_switch_selected_account_ids,
        quota_alert_enabled: user_config.quota_alert_enabled,
        quota_alert_threshold: user_config.quota_alert_threshold,
        codex_quota_alert_enabled: user_config.codex_quota_alert_enabled,
        codex_quota_alert_threshold: user_config.codex_quota_alert_threshold,
        zed_quota_alert_enabled: user_config.zed_quota_alert_enabled,
        zed_quota_alert_threshold: user_config.zed_quota_alert_threshold,
        codex_quota_alert_primary_threshold: user_config.codex_quota_alert_primary_threshold,
        codex_quota_alert_secondary_threshold: user_config.codex_quota_alert_secondary_threshold,
        ghcp_quota_alert_enabled: user_config.ghcp_quota_alert_enabled,
        ghcp_quota_alert_threshold: user_config.ghcp_quota_alert_threshold,
        windsurf_quota_alert_enabled: user_config.windsurf_quota_alert_enabled,
        windsurf_quota_alert_threshold: user_config.windsurf_quota_alert_threshold,
        kiro_quota_alert_enabled: user_config.kiro_quota_alert_enabled,
        kiro_quota_alert_threshold: user_config.kiro_quota_alert_threshold,
        cursor_quota_alert_enabled: user_config.cursor_quota_alert_enabled,
        cursor_quota_alert_threshold: user_config.cursor_quota_alert_threshold,
        gemini_quota_alert_enabled: user_config.gemini_quota_alert_enabled,
        gemini_quota_alert_threshold: user_config.gemini_quota_alert_threshold,
        codebuddy_quota_alert_enabled: user_config.codebuddy_quota_alert_enabled,
        codebuddy_quota_alert_threshold: user_config.codebuddy_quota_alert_threshold,
        codebuddy_cn_quota_alert_enabled: user_config.codebuddy_cn_quota_alert_enabled,
        codebuddy_cn_quota_alert_threshold: user_config.codebuddy_cn_quota_alert_threshold,
        qoder_quota_alert_enabled: user_config.qoder_quota_alert_enabled,
        qoder_quota_alert_threshold: user_config.qoder_quota_alert_threshold,
        trae_quota_alert_enabled: user_config.trae_quota_alert_enabled,
        trae_quota_alert_threshold: user_config.trae_quota_alert_threshold,
        workbuddy_quota_alert_enabled: user_config.workbuddy_quota_alert_enabled,
        workbuddy_quota_alert_threshold: user_config.workbuddy_quota_alert_threshold,
    }
}

pub fn get_general_config(app_auto_launch_enabled_override: Option<bool>) -> GeneralConfig {
    general_config_from_user_config(get_user_config(), app_auto_launch_enabled_override)
}

fn camel_to_snake_case(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

fn trim_config_path(value: String) -> String {
    value.trim().to_string()
}

fn normalize_general_config_update(current: &UserConfig, mut next: UserConfig) -> UserConfig {
    next.language = next.language.to_lowercase();
    next.ui_scale = sanitize_ui_scale(next.ui_scale);
    next.codex_wsl_config_dir = trim_config_path(next.codex_wsl_config_dir);
    next.opencode_app_path = trim_config_path(next.opencode_app_path);
    next.antigravity_app_path = trim_config_path(next.antigravity_app_path);
    next.codex_app_path = trim_config_path(next.codex_app_path);
    next.codex_specified_app_path = trim_config_path(next.codex_specified_app_path);
    next.zed_app_path = trim_config_path(next.zed_app_path);
    next.vscode_app_path = trim_config_path(next.vscode_app_path);
    next.windsurf_app_path = trim_config_path(next.windsurf_app_path);
    next.kiro_app_path = trim_config_path(next.kiro_app_path);
    next.cursor_app_path = trim_config_path(next.cursor_app_path);
    next.codebuddy_app_path = trim_config_path(next.codebuddy_app_path);
    next.codebuddy_cn_app_path = trim_config_path(next.codebuddy_cn_app_path);
    next.qoder_app_path = trim_config_path(next.qoder_app_path);
    next.trae_app_path = trim_config_path(next.trae_app_path);
    next.workbuddy_app_path = trim_config_path(next.workbuddy_app_path);
    next.antigravity_startup_wakeup_delay_seconds =
        sanitize_startup_wakeup_delay_seconds(next.antigravity_startup_wakeup_delay_seconds);
    next.codex_startup_wakeup_delay_seconds =
        sanitize_startup_wakeup_delay_seconds(next.codex_startup_wakeup_delay_seconds);

    if !next.opencode_auth_overwrite_on_switch {
        next.opencode_sync_on_switch = false;
    }
    if !next.ghcp_opencode_auth_overwrite_on_switch {
        next.ghcp_opencode_sync_on_switch = false;
    }

    let auto_switch_scope_mode = next.auto_switch_scope_mode.trim().to_string();
    next.auto_switch_scope_mode = if auto_switch_scope_mode.is_empty() {
        current.auto_switch_scope_mode.clone()
    } else {
        auto_switch_scope_mode
    };
    next.auto_switch_account_scope_mode =
        normalize_auto_switch_account_scope_mode(&next.auto_switch_account_scope_mode);
    next.auto_switch_selected_account_ids =
        normalize_auto_switch_selected_account_ids(&next.auto_switch_selected_account_ids);
    next.codex_auto_switch_account_scope_mode =
        normalize_auto_switch_account_scope_mode(&next.codex_auto_switch_account_scope_mode);
    next.codex_auto_switch_selected_account_ids =
        normalize_auto_switch_selected_account_ids(&next.codex_auto_switch_selected_account_ids);

    next
}

pub fn resolve_general_config_save(
    current: UserConfig,
    input: GeneralConfigSaveInput,
) -> UserConfig {
    let mut next = current.clone();

    let next_codex_quota_alert_threshold = input
        .codex_quota_alert_threshold
        .unwrap_or(current.codex_quota_alert_threshold);
    let next_opencode_auth_overwrite_on_switch = input
        .opencode_auth_overwrite_on_switch
        .unwrap_or(current.opencode_auth_overwrite_on_switch);
    let next_opencode_sync_on_switch = if next_opencode_auth_overwrite_on_switch {
        input.opencode_sync_on_switch
    } else {
        false
    };
    let next_ghcp_opencode_auth_overwrite_on_switch = input
        .ghcp_opencode_auth_overwrite_on_switch
        .unwrap_or(current.ghcp_opencode_auth_overwrite_on_switch);
    let next_ghcp_opencode_sync_on_switch = if next_ghcp_opencode_auth_overwrite_on_switch {
        input
            .ghcp_opencode_sync_on_switch
            .unwrap_or(current.ghcp_opencode_sync_on_switch)
    } else {
        false
    };

    next.language = input.language.to_lowercase();
    next.default_terminal = input
        .default_terminal
        .unwrap_or_else(|| current.default_terminal.clone());
    next.theme = input.theme;
    next.ui_scale = sanitize_ui_scale(input.ui_scale.unwrap_or(current.ui_scale));
    next.auto_refresh_minutes = input.auto_refresh_minutes;
    next.codex_auto_refresh_minutes = input.codex_auto_refresh_minutes;
    next.codex_sync_wsl = input.codex_sync_wsl.unwrap_or(current.codex_sync_wsl);
    next.codex_wsl_config_dir = input
        .codex_wsl_config_dir
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.codex_wsl_config_dir.clone());
    next.zed_auto_refresh_minutes = input
        .zed_auto_refresh_minutes
        .unwrap_or(current.zed_auto_refresh_minutes);
    next.ghcp_auto_refresh_minutes = input
        .ghcp_auto_refresh_minutes
        .unwrap_or(current.ghcp_auto_refresh_minutes);
    next.windsurf_auto_refresh_minutes = input
        .windsurf_auto_refresh_minutes
        .unwrap_or(current.windsurf_auto_refresh_minutes);
    next.kiro_auto_refresh_minutes = input
        .kiro_auto_refresh_minutes
        .unwrap_or(current.kiro_auto_refresh_minutes);
    next.cursor_auto_refresh_minutes = input
        .cursor_auto_refresh_minutes
        .unwrap_or(current.cursor_auto_refresh_minutes);
    next.gemini_auto_refresh_minutes = input
        .gemini_auto_refresh_minutes
        .unwrap_or(current.gemini_auto_refresh_minutes);
    next.gemini_sync_wsl = input.gemini_sync_wsl.unwrap_or(current.gemini_sync_wsl);
    next.codebuddy_auto_refresh_minutes = input
        .codebuddy_auto_refresh_minutes
        .unwrap_or(current.codebuddy_auto_refresh_minutes);
    next.codebuddy_cn_auto_refresh_minutes = input
        .codebuddy_cn_auto_refresh_minutes
        .unwrap_or(current.codebuddy_cn_auto_refresh_minutes);
    next.workbuddy_auto_refresh_minutes = input
        .workbuddy_auto_refresh_minutes
        .unwrap_or(current.workbuddy_auto_refresh_minutes);
    next.qoder_auto_refresh_minutes = input
        .qoder_auto_refresh_minutes
        .unwrap_or(current.qoder_auto_refresh_minutes);
    next.trae_auto_refresh_minutes = input
        .trae_auto_refresh_minutes
        .unwrap_or(current.trae_auto_refresh_minutes);
    next.close_behavior = match input.close_behavior.as_str() {
        "minimize" => CloseWindowBehavior::Minimize,
        "quit" => CloseWindowBehavior::Quit,
        _ => CloseWindowBehavior::Ask,
    };
    next.minimize_behavior = match input.minimize_behavior.as_deref() {
        Some("dock_and_tray") => MinimizeWindowBehavior::DockAndTray,
        Some("tray_only") => MinimizeWindowBehavior::TrayOnly,
        Some(_) | None => current.minimize_behavior.clone(),
    };
    next.hide_dock_icon = input.hide_dock_icon.unwrap_or(current.hide_dock_icon);
    next.tray_icon_style = input
        .tray_icon_style
        .as_deref()
        .map(TrayIconStyle::from_str)
        .unwrap_or(current.tray_icon_style);
    next.floating_card_show_on_startup = input
        .floating_card_show_on_startup
        .unwrap_or(current.floating_card_show_on_startup);
    next.floating_card_always_on_top = input
        .floating_card_always_on_top
        .unwrap_or(current.floating_card_always_on_top);
    next.app_auto_launch_enabled = input
        .app_auto_launch_enabled
        .unwrap_or(current.app_auto_launch_enabled);
    next.antigravity_startup_wakeup_enabled = input
        .antigravity_startup_wakeup_enabled
        .unwrap_or(current.antigravity_startup_wakeup_enabled);
    next.antigravity_startup_wakeup_delay_seconds = sanitize_startup_wakeup_delay_seconds(
        input
            .antigravity_startup_wakeup_delay_seconds
            .unwrap_or(current.antigravity_startup_wakeup_delay_seconds),
    );
    next.codex_startup_wakeup_enabled = input
        .codex_startup_wakeup_enabled
        .unwrap_or(current.codex_startup_wakeup_enabled);
    next.codex_startup_wakeup_delay_seconds = sanitize_startup_wakeup_delay_seconds(
        input
            .codex_startup_wakeup_delay_seconds
            .unwrap_or(current.codex_startup_wakeup_delay_seconds),
    );
    next.floating_card_confirm_on_close = input
        .floating_card_confirm_on_close
        .unwrap_or(current.floating_card_confirm_on_close);
    next.opencode_app_path = input.opencode_app_path.trim().to_string();
    next.antigravity_app_path = input.antigravity_app_path.trim().to_string();
    next.codex_app_path = input.codex_app_path.trim().to_string();
    next.codex_specified_app_path = input
        .codex_specified_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.codex_specified_app_path.clone());
    next.zed_app_path = input
        .zed_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.zed_app_path.clone());
    next.vscode_app_path = input.vscode_app_path.trim().to_string();
    next.windsurf_app_path = input
        .windsurf_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.windsurf_app_path.clone());
    next.kiro_app_path = input
        .kiro_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.kiro_app_path.clone());
    next.cursor_app_path = input
        .cursor_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.cursor_app_path.clone());
    next.codebuddy_app_path = input
        .codebuddy_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.codebuddy_app_path.clone());
    next.codebuddy_cn_app_path = input
        .codebuddy_cn_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.codebuddy_cn_app_path.clone());
    next.qoder_app_path = input
        .qoder_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.qoder_app_path.clone());
    next.trae_app_path = input
        .trae_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.trae_app_path.clone());
    next.workbuddy_app_path = input
        .workbuddy_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.workbuddy_app_path.clone());
    next.opencode_sync_on_switch = next_opencode_sync_on_switch;
    next.opencode_auth_overwrite_on_switch = next_opencode_auth_overwrite_on_switch;
    next.ghcp_opencode_sync_on_switch = next_ghcp_opencode_sync_on_switch;
    next.ghcp_opencode_auth_overwrite_on_switch = next_ghcp_opencode_auth_overwrite_on_switch;
    next.ghcp_launch_on_switch = input
        .ghcp_launch_on_switch
        .unwrap_or(current.ghcp_launch_on_switch);
    next.openclaw_auth_overwrite_on_switch = input
        .openclaw_auth_overwrite_on_switch
        .unwrap_or(current.openclaw_auth_overwrite_on_switch);
    next.codex_launch_on_switch = input.codex_launch_on_switch;
    next.codex_restart_specified_app_on_switch = input
        .codex_restart_specified_app_on_switch
        .unwrap_or(current.codex_restart_specified_app_on_switch);
    next.codex_local_access_entry_visible = input
        .codex_local_access_entry_visible
        .unwrap_or(current.codex_local_access_entry_visible);
    next.top_right_ad_visible = input
        .top_right_ad_visible
        .unwrap_or(current.top_right_ad_visible);
    next.antigravity_dual_switch_no_restart_enabled = input
        .antigravity_dual_switch_no_restart_enabled
        .unwrap_or(current.antigravity_dual_switch_no_restart_enabled);
    next.auto_switch_enabled = input
        .auto_switch_enabled
        .unwrap_or(current.auto_switch_enabled);
    next.auto_switch_threshold = input
        .auto_switch_threshold
        .unwrap_or(current.auto_switch_threshold);
    next.auto_switch_credits_enabled = input
        .auto_switch_credits_enabled
        .unwrap_or(current.auto_switch_credits_enabled);
    next.auto_switch_credits_threshold = input
        .auto_switch_credits_threshold
        .unwrap_or(current.auto_switch_credits_threshold);
    next.auto_switch_scope_mode = input
        .auto_switch_scope_mode
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| current.auto_switch_scope_mode.clone());
    next.auto_switch_selected_group_ids = input
        .auto_switch_selected_group_ids
        .unwrap_or_else(|| current.auto_switch_selected_group_ids.clone());
    next.auto_switch_account_scope_mode = normalize_auto_switch_account_scope_mode(
        input
            .auto_switch_account_scope_mode
            .as_deref()
            .unwrap_or(current.auto_switch_account_scope_mode.as_str()),
    );
    next.auto_switch_selected_account_ids = normalize_auto_switch_selected_account_ids(
        input
            .auto_switch_selected_account_ids
            .as_deref()
            .unwrap_or(current.auto_switch_selected_account_ids.as_slice()),
    );
    next.codex_auto_switch_enabled = input
        .codex_auto_switch_enabled
        .unwrap_or(current.codex_auto_switch_enabled);
    next.codex_auto_switch_primary_threshold = input
        .codex_auto_switch_primary_threshold
        .unwrap_or(current.codex_auto_switch_primary_threshold);
    next.codex_auto_switch_secondary_threshold = input
        .codex_auto_switch_secondary_threshold
        .unwrap_or(current.codex_auto_switch_secondary_threshold);
    next.codex_auto_switch_account_scope_mode = normalize_auto_switch_account_scope_mode(
        input
            .codex_auto_switch_account_scope_mode
            .as_deref()
            .unwrap_or(current.codex_auto_switch_account_scope_mode.as_str()),
    );
    next.codex_auto_switch_selected_account_ids = normalize_auto_switch_selected_account_ids(
        input
            .codex_auto_switch_selected_account_ids
            .as_deref()
            .unwrap_or(current.codex_auto_switch_selected_account_ids.as_slice()),
    );
    next.quota_alert_enabled = input
        .quota_alert_enabled
        .unwrap_or(current.quota_alert_enabled);
    next.quota_alert_threshold = input
        .quota_alert_threshold
        .unwrap_or(current.quota_alert_threshold);
    next.codex_quota_alert_enabled = input
        .codex_quota_alert_enabled
        .unwrap_or(current.codex_quota_alert_enabled);
    next.codex_quota_alert_threshold = next_codex_quota_alert_threshold;
    next.zed_quota_alert_enabled = input
        .zed_quota_alert_enabled
        .unwrap_or(current.zed_quota_alert_enabled);
    next.zed_quota_alert_threshold = input
        .zed_quota_alert_threshold
        .unwrap_or(current.zed_quota_alert_threshold);
    next.codex_quota_alert_primary_threshold = input
        .codex_quota_alert_primary_threshold
        .unwrap_or(next_codex_quota_alert_threshold);
    next.codex_quota_alert_secondary_threshold = input
        .codex_quota_alert_secondary_threshold
        .unwrap_or(next_codex_quota_alert_threshold);
    next.ghcp_quota_alert_enabled = input
        .ghcp_quota_alert_enabled
        .unwrap_or(current.ghcp_quota_alert_enabled);
    next.ghcp_quota_alert_threshold = input
        .ghcp_quota_alert_threshold
        .unwrap_or(current.ghcp_quota_alert_threshold);
    next.windsurf_quota_alert_enabled = input
        .windsurf_quota_alert_enabled
        .unwrap_or(current.windsurf_quota_alert_enabled);
    next.windsurf_quota_alert_threshold = input
        .windsurf_quota_alert_threshold
        .unwrap_or(current.windsurf_quota_alert_threshold);
    next.kiro_quota_alert_enabled = input
        .kiro_quota_alert_enabled
        .unwrap_or(current.kiro_quota_alert_enabled);
    next.kiro_quota_alert_threshold = input
        .kiro_quota_alert_threshold
        .unwrap_or(current.kiro_quota_alert_threshold);
    next.cursor_quota_alert_enabled = input
        .cursor_quota_alert_enabled
        .unwrap_or(current.cursor_quota_alert_enabled);
    next.cursor_quota_alert_threshold = input
        .cursor_quota_alert_threshold
        .unwrap_or(current.cursor_quota_alert_threshold);
    next.gemini_quota_alert_enabled = input
        .gemini_quota_alert_enabled
        .unwrap_or(current.gemini_quota_alert_enabled);
    next.gemini_quota_alert_threshold = input
        .gemini_quota_alert_threshold
        .unwrap_or(current.gemini_quota_alert_threshold);
    next.codebuddy_quota_alert_enabled = input
        .codebuddy_quota_alert_enabled
        .unwrap_or(current.codebuddy_quota_alert_enabled);
    next.codebuddy_quota_alert_threshold = input
        .codebuddy_quota_alert_threshold
        .unwrap_or(current.codebuddy_quota_alert_threshold);
    next.codebuddy_cn_quota_alert_enabled = input
        .codebuddy_cn_quota_alert_enabled
        .unwrap_or(current.codebuddy_cn_quota_alert_enabled);
    next.codebuddy_cn_quota_alert_threshold = input
        .codebuddy_cn_quota_alert_threshold
        .unwrap_or(current.codebuddy_cn_quota_alert_threshold);
    next.qoder_quota_alert_enabled = input
        .qoder_quota_alert_enabled
        .unwrap_or(current.qoder_quota_alert_enabled);
    next.qoder_quota_alert_threshold = input
        .qoder_quota_alert_threshold
        .unwrap_or(current.qoder_quota_alert_threshold);
    next.trae_quota_alert_enabled = input
        .trae_quota_alert_enabled
        .unwrap_or(current.trae_quota_alert_enabled);
    next.trae_quota_alert_threshold = input
        .trae_quota_alert_threshold
        .unwrap_or(current.trae_quota_alert_threshold);
    next.workbuddy_quota_alert_enabled = input
        .workbuddy_quota_alert_enabled
        .unwrap_or(current.workbuddy_quota_alert_enabled);
    next.workbuddy_quota_alert_threshold = input
        .workbuddy_quota_alert_threshold
        .unwrap_or(current.workbuddy_quota_alert_threshold);

    next
}

pub fn resolve_general_config_patch(
    current: UserConfig,
    params: &Value,
) -> Result<UserConfig, String> {
    let params = params
        .as_object()
        .and_then(|object| object.get("config"))
        .unwrap_or(params);
    let Some(params_object) = params.as_object() else {
        return Err("params must be an object".to_string());
    };

    let allowed_value =
        serde_json::to_value(general_config_from_user_config(current.clone(), None))
            .map_err(|err| format!("serialize general config failed: {err}"))?;
    let allowed = allowed_value
        .as_object()
        .ok_or_else(|| "general config shape is invalid".to_string())?;
    let mut next_value = serde_json::to_value(&current)
        .map_err(|err| format!("serialize current config failed: {err}"))?;
    let next_object = next_value
        .as_object_mut()
        .ok_or_else(|| "current config shape is invalid".to_string())?;
    let mut codex_quota_alert_threshold_updated = false;
    let mut codex_quota_alert_primary_threshold_updated = false;
    let mut codex_quota_alert_secondary_threshold_updated = false;

    for (raw_key, value) in params_object {
        if value.is_null() {
            continue;
        }
        let key = if allowed.contains_key(raw_key) {
            raw_key.clone()
        } else {
            camel_to_snake_case(raw_key)
        };
        if allowed.contains_key(&key) {
            match key.as_str() {
                "codex_quota_alert_threshold" => codex_quota_alert_threshold_updated = true,
                "codex_quota_alert_primary_threshold" => {
                    codex_quota_alert_primary_threshold_updated = true
                }
                "codex_quota_alert_secondary_threshold" => {
                    codex_quota_alert_secondary_threshold_updated = true
                }
                _ => {}
            }
            next_object.insert(key, value.clone());
        }
    }

    let mut next: UserConfig = serde_json::from_value(next_value)
        .map_err(|err| format!("invalid settings payload: {err}"))?;
    if codex_quota_alert_threshold_updated {
        let threshold = next.codex_quota_alert_threshold;
        if !codex_quota_alert_primary_threshold_updated {
            next.codex_quota_alert_primary_threshold = threshold;
        }
        if !codex_quota_alert_secondary_threshold_updated {
            next.codex_quota_alert_secondary_threshold = threshold;
        }
    }
    Ok(normalize_general_config_update(&current, next))
}

pub fn save_general_config_patch(params: &Value) -> Result<(), String> {
    let current = get_user_config();
    let next = resolve_general_config_patch(current, params)?;
    save_user_config(&next)
}

pub fn resolve_set_app_path(
    mut current: UserConfig,
    app: &str,
    path: impl AsRef<str>,
) -> Result<UserConfig, String> {
    let normalized_path = path.as_ref().trim().to_string();
    match app {
        "antigravity" => current.antigravity_app_path = normalized_path,
        "codex" => current.codex_app_path = normalized_path,
        "zed" => current.zed_app_path = normalized_path,
        "vscode" => current.vscode_app_path = normalized_path,
        "windsurf" => current.windsurf_app_path = normalized_path,
        "kiro" => current.kiro_app_path = normalized_path,
        "cursor" => current.cursor_app_path = normalized_path,
        "codebuddy" => current.codebuddy_app_path = normalized_path,
        "codebuddy_cn" => current.codebuddy_cn_app_path = normalized_path,
        "qoder" => current.qoder_app_path = normalized_path,
        "trae" => current.trae_app_path = normalized_path,
        "workbuddy" => current.workbuddy_app_path = normalized_path,
        "opencode" => current.opencode_app_path = normalized_path,
        _ => return Err("未知应用类型".to_string()),
    }
    Ok(current)
}

pub fn set_app_path(app: &str, path: impl AsRef<str>) -> Result<(), String> {
    let current = get_user_config();
    let next = resolve_set_app_path(current, app, path)?;
    save_user_config(&next)
}

fn resolve_set_codex_launch_on_switch(
    mut current: UserConfig,
    enabled: bool,
) -> (UserConfig, bool) {
    let changed = current.codex_launch_on_switch != enabled;
    current.codex_launch_on_switch = enabled;
    (current, changed)
}

pub fn set_codex_launch_on_switch(enabled: bool) -> Result<(), String> {
    let (next, changed) = resolve_set_codex_launch_on_switch(get_user_config(), enabled);
    if !changed {
        return Ok(());
    }
    save_user_config(&next)
}

fn resolve_set_codex_local_access_entry_visible(
    mut current: UserConfig,
    enabled: bool,
) -> (UserConfig, bool) {
    let changed = current.codex_local_access_entry_visible != enabled;
    current.codex_local_access_entry_visible = enabled;
    (current, changed)
}

pub fn set_codex_local_access_entry_visible(enabled: bool) -> Result<(), String> {
    let (next, changed) = resolve_set_codex_local_access_entry_visible(get_user_config(), enabled);
    if !changed {
        return Ok(());
    }
    save_user_config(&next)
}

pub fn close_behavior_from_window_action(action: &str) -> CloseWindowBehavior {
    match action {
        "minimize" => CloseWindowBehavior::Minimize,
        "quit" => CloseWindowBehavior::Quit,
        _ => CloseWindowBehavior::Ask,
    }
}

fn resolve_save_close_behavior_for_window_action(
    mut current: UserConfig,
    action: &str,
) -> (UserConfig, bool) {
    let close_behavior = close_behavior_from_window_action(action);
    let changed = current.close_behavior != close_behavior;
    current.close_behavior = close_behavior;
    (current, changed)
}

pub fn save_close_behavior_for_window_action(action: &str) -> Result<(), String> {
    let (next, changed) = resolve_save_close_behavior_for_window_action(get_user_config(), action);
    if !changed {
        return Ok(());
    }
    save_user_config(&next)
}

fn resolve_set_floating_card_always_on_top(
    mut current: UserConfig,
    always_on_top: bool,
) -> (UserConfig, bool) {
    let changed = current.floating_card_always_on_top != always_on_top;
    current.floating_card_always_on_top = always_on_top;
    (current, changed)
}

pub fn set_floating_card_always_on_top(always_on_top: bool) -> Result<(), String> {
    let (next, changed) = resolve_set_floating_card_always_on_top(get_user_config(), always_on_top);
    if !changed {
        return Ok(());
    }
    save_user_config(&next)
}

fn resolve_set_floating_card_confirm_on_close(
    mut current: UserConfig,
    confirm_on_close: bool,
) -> (UserConfig, bool) {
    let changed = current.floating_card_confirm_on_close != confirm_on_close;
    current.floating_card_confirm_on_close = confirm_on_close;
    (current, changed)
}

pub fn set_floating_card_confirm_on_close(confirm_on_close: bool) -> Result<(), String> {
    let (next, changed) =
        resolve_set_floating_card_confirm_on_close(get_user_config(), confirm_on_close);
    if !changed {
        return Ok(());
    }
    save_user_config(&next)
}

fn resolve_save_floating_card_position(
    mut current: UserConfig,
    x: i32,
    y: i32,
) -> (UserConfig, bool) {
    let changed =
        current.floating_card_position_x != Some(x) || current.floating_card_position_y != Some(y);
    current.floating_card_position_x = Some(x);
    current.floating_card_position_y = Some(y);
    (current, changed)
}

pub fn save_floating_card_position(x: i32, y: i32) -> Result<(), String> {
    let (next, changed) = resolve_save_floating_card_position(get_user_config(), x, y);
    if !changed {
        return Ok(());
    }
    save_user_config(&next)
}

pub fn get_network_config(report_actual_port: Option<u16>) -> NetworkConfig {
    let config = get_user_config();
    NetworkConfig {
        ws_enabled: config.ws_enabled,
        ws_port: config.ws_port,
        actual_port: get_actual_port(),
        default_port: DEFAULT_WS_PORT,
        report_enabled: config.report_enabled,
        report_port: config.report_port,
        report_actual_port,
        report_default_port: DEFAULT_REPORT_PORT,
        report_token: config.report_token,
        global_proxy_enabled: config.global_proxy_enabled,
        global_proxy_url: config.global_proxy_url,
        global_proxy_no_proxy: config.global_proxy_no_proxy,
    }
}

fn resolve_network_config_update(
    current: UserConfig,
    update: NetworkConfigUpdate,
) -> Result<(UserConfig, bool), String> {
    let mut next = current.clone();
    next.ws_enabled = update.ws_enabled;
    next.ws_port = update.ws_port;
    next.report_enabled = update.report_enabled.unwrap_or(current.report_enabled);
    next.report_port = update.report_port.unwrap_or(current.report_port);
    next.report_token = update
        .report_token
        .unwrap_or_else(|| current.report_token.clone())
        .trim()
        .to_string();
    next.global_proxy_enabled = update
        .global_proxy_enabled
        .unwrap_or(current.global_proxy_enabled);
    next.global_proxy_url = update
        .global_proxy_url
        .unwrap_or_else(|| current.global_proxy_url.clone())
        .trim()
        .to_string();
    next.global_proxy_no_proxy = update
        .global_proxy_no_proxy
        .unwrap_or_else(|| current.global_proxy_no_proxy.clone())
        .trim()
        .to_string();

    if next.report_enabled && next.report_token.is_empty() {
        return Err("网页查询服务 token 不能为空".to_string());
    }
    if next.global_proxy_enabled && next.global_proxy_url.is_empty() {
        return Err("启用全局代理时，代理地址不能为空".to_string());
    }

    let needs_restart = current.ws_port != next.ws_port
        || current.ws_enabled != next.ws_enabled
        || current.report_enabled != next.report_enabled
        || current.report_port != next.report_port
        || current.report_token != next.report_token;

    Ok((next, needs_restart))
}

pub fn save_network_config(update: NetworkConfigUpdate) -> Result<bool, String> {
    let current = get_user_config();
    let (next, needs_restart) = resolve_network_config_update(current, update)?;
    save_user_config(&next)?;
    Ok(needs_restart)
}

/// 保存服务状态到共享文件
pub fn save_server_status(status: &ServerStatus) -> Result<(), String> {
    let status_path = get_server_status_path()?;
    let data_dir = get_data_dir()?;

    // 确保目录存在
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }

    // 写入状态文件
    let json =
        serde_json::to_string_pretty(status).map_err(|e| format!("序列化状态失败: {}", e))?;

    crate::modules::atomic_write::write_string_atomic(&status_path, &json)
        .map_err(|e| format!("写入状态文件失败: {}", e))?;

    crate::modules::logger::log_info(&format!(
        "[Config] 服务状态已保存: ws_port={}, pid={}",
        status.ws_port, status.pid
    ));

    Ok(())
}

/// 初始化服务状态（WebSocket 启动后调用）
pub fn init_server_status(actual_port: u16) -> Result<(), String> {
    // 更新运行时状态
    if let Ok(mut state) = get_runtime_state().write() {
        state.actual_port = Some(actual_port);
    }

    let status = ServerStatus {
        ws_port: actual_port,
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        started_at: chrono::Utc::now().timestamp(),
    };

    save_server_status(&status)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        close_behavior_from_window_action, general_config_from_user_config,
        resolve_general_config_save, resolve_network_config_update,
        resolve_save_close_behavior_for_window_action, resolve_save_floating_card_position,
        resolve_set_app_path, resolve_set_codex_launch_on_switch,
        resolve_set_codex_local_access_entry_visible, resolve_set_floating_card_always_on_top,
        resolve_set_floating_card_confirm_on_close, CloseWindowBehavior, GeneralConfigSaveInput,
        MinimizeWindowBehavior, NetworkConfigUpdate, TrayIconStyle, UserConfig,
    };

    fn default_general_save_input(current: &UserConfig) -> GeneralConfigSaveInput {
        GeneralConfigSaveInput {
            language: current.language.clone(),
            default_terminal: Some(current.default_terminal.clone()),
            theme: current.theme.clone(),
            ui_scale: Some(current.ui_scale),
            auto_refresh_minutes: current.auto_refresh_minutes,
            codex_auto_refresh_minutes: current.codex_auto_refresh_minutes,
            codex_sync_wsl: Some(current.codex_sync_wsl),
            codex_wsl_config_dir: Some(current.codex_wsl_config_dir.clone()),
            zed_auto_refresh_minutes: Some(current.zed_auto_refresh_minutes),
            ghcp_auto_refresh_minutes: Some(current.ghcp_auto_refresh_minutes),
            windsurf_auto_refresh_minutes: Some(current.windsurf_auto_refresh_minutes),
            kiro_auto_refresh_minutes: Some(current.kiro_auto_refresh_minutes),
            cursor_auto_refresh_minutes: Some(current.cursor_auto_refresh_minutes),
            gemini_auto_refresh_minutes: Some(current.gemini_auto_refresh_minutes),
            gemini_sync_wsl: Some(current.gemini_sync_wsl),
            codebuddy_auto_refresh_minutes: Some(current.codebuddy_auto_refresh_minutes),
            codebuddy_cn_auto_refresh_minutes: Some(current.codebuddy_cn_auto_refresh_minutes),
            workbuddy_auto_refresh_minutes: Some(current.workbuddy_auto_refresh_minutes),
            qoder_auto_refresh_minutes: Some(current.qoder_auto_refresh_minutes),
            trae_auto_refresh_minutes: Some(current.trae_auto_refresh_minutes),
            close_behavior: "ask".to_string(),
            minimize_behavior: Some("dock_and_tray".to_string()),
            hide_dock_icon: Some(current.hide_dock_icon),
            tray_icon_style: Some(current.tray_icon_style.as_str().to_string()),
            floating_card_show_on_startup: Some(current.floating_card_show_on_startup),
            floating_card_always_on_top: Some(current.floating_card_always_on_top),
            app_auto_launch_enabled: Some(current.app_auto_launch_enabled),
            antigravity_startup_wakeup_enabled: Some(current.antigravity_startup_wakeup_enabled),
            antigravity_startup_wakeup_delay_seconds: Some(
                current.antigravity_startup_wakeup_delay_seconds,
            ),
            codex_startup_wakeup_enabled: Some(current.codex_startup_wakeup_enabled),
            codex_startup_wakeup_delay_seconds: Some(current.codex_startup_wakeup_delay_seconds),
            floating_card_confirm_on_close: Some(current.floating_card_confirm_on_close),
            opencode_app_path: current.opencode_app_path.clone(),
            antigravity_app_path: current.antigravity_app_path.clone(),
            codex_app_path: current.codex_app_path.clone(),
            codex_specified_app_path: Some(current.codex_specified_app_path.clone()),
            zed_app_path: Some(current.zed_app_path.clone()),
            vscode_app_path: current.vscode_app_path.clone(),
            windsurf_app_path: Some(current.windsurf_app_path.clone()),
            kiro_app_path: Some(current.kiro_app_path.clone()),
            cursor_app_path: Some(current.cursor_app_path.clone()),
            codebuddy_app_path: Some(current.codebuddy_app_path.clone()),
            codebuddy_cn_app_path: Some(current.codebuddy_cn_app_path.clone()),
            qoder_app_path: Some(current.qoder_app_path.clone()),
            trae_app_path: Some(current.trae_app_path.clone()),
            workbuddy_app_path: Some(current.workbuddy_app_path.clone()),
            opencode_sync_on_switch: current.opencode_sync_on_switch,
            opencode_auth_overwrite_on_switch: Some(current.opencode_auth_overwrite_on_switch),
            ghcp_opencode_sync_on_switch: Some(current.ghcp_opencode_sync_on_switch),
            ghcp_opencode_auth_overwrite_on_switch: Some(
                current.ghcp_opencode_auth_overwrite_on_switch,
            ),
            ghcp_launch_on_switch: Some(current.ghcp_launch_on_switch),
            openclaw_auth_overwrite_on_switch: Some(current.openclaw_auth_overwrite_on_switch),
            codex_launch_on_switch: current.codex_launch_on_switch,
            codex_restart_specified_app_on_switch: Some(
                current.codex_restart_specified_app_on_switch,
            ),
            codex_local_access_entry_visible: Some(current.codex_local_access_entry_visible),
            top_right_ad_visible: Some(current.top_right_ad_visible),
            antigravity_dual_switch_no_restart_enabled: Some(
                current.antigravity_dual_switch_no_restart_enabled,
            ),
            auto_switch_enabled: Some(current.auto_switch_enabled),
            auto_switch_threshold: Some(current.auto_switch_threshold),
            auto_switch_credits_enabled: Some(current.auto_switch_credits_enabled),
            auto_switch_credits_threshold: Some(current.auto_switch_credits_threshold),
            auto_switch_scope_mode: Some(current.auto_switch_scope_mode.clone()),
            auto_switch_selected_group_ids: Some(current.auto_switch_selected_group_ids.clone()),
            auto_switch_account_scope_mode: Some(current.auto_switch_account_scope_mode.clone()),
            auto_switch_selected_account_ids: Some(
                current.auto_switch_selected_account_ids.clone(),
            ),
            codex_auto_switch_enabled: Some(current.codex_auto_switch_enabled),
            codex_auto_switch_primary_threshold: Some(current.codex_auto_switch_primary_threshold),
            codex_auto_switch_secondary_threshold: Some(
                current.codex_auto_switch_secondary_threshold,
            ),
            codex_auto_switch_account_scope_mode: Some(
                current.codex_auto_switch_account_scope_mode.clone(),
            ),
            codex_auto_switch_selected_account_ids: Some(
                current.codex_auto_switch_selected_account_ids.clone(),
            ),
            quota_alert_enabled: Some(current.quota_alert_enabled),
            quota_alert_threshold: Some(current.quota_alert_threshold),
            codex_quota_alert_enabled: Some(current.codex_quota_alert_enabled),
            codex_quota_alert_threshold: Some(current.codex_quota_alert_threshold),
            zed_quota_alert_enabled: Some(current.zed_quota_alert_enabled),
            zed_quota_alert_threshold: Some(current.zed_quota_alert_threshold),
            codex_quota_alert_primary_threshold: Some(current.codex_quota_alert_primary_threshold),
            codex_quota_alert_secondary_threshold: Some(
                current.codex_quota_alert_secondary_threshold,
            ),
            ghcp_quota_alert_enabled: Some(current.ghcp_quota_alert_enabled),
            ghcp_quota_alert_threshold: Some(current.ghcp_quota_alert_threshold),
            windsurf_quota_alert_enabled: Some(current.windsurf_quota_alert_enabled),
            windsurf_quota_alert_threshold: Some(current.windsurf_quota_alert_threshold),
            kiro_quota_alert_enabled: Some(current.kiro_quota_alert_enabled),
            kiro_quota_alert_threshold: Some(current.kiro_quota_alert_threshold),
            cursor_quota_alert_enabled: Some(current.cursor_quota_alert_enabled),
            cursor_quota_alert_threshold: Some(current.cursor_quota_alert_threshold),
            gemini_quota_alert_enabled: Some(current.gemini_quota_alert_enabled),
            gemini_quota_alert_threshold: Some(current.gemini_quota_alert_threshold),
            codebuddy_quota_alert_enabled: Some(current.codebuddy_quota_alert_enabled),
            codebuddy_quota_alert_threshold: Some(current.codebuddy_quota_alert_threshold),
            codebuddy_cn_quota_alert_enabled: Some(current.codebuddy_cn_quota_alert_enabled),
            codebuddy_cn_quota_alert_threshold: Some(current.codebuddy_cn_quota_alert_threshold),
            qoder_quota_alert_enabled: Some(current.qoder_quota_alert_enabled),
            qoder_quota_alert_threshold: Some(current.qoder_quota_alert_threshold),
            trae_quota_alert_enabled: Some(current.trae_quota_alert_enabled),
            trae_quota_alert_threshold: Some(current.trae_quota_alert_threshold),
            workbuddy_quota_alert_enabled: Some(current.workbuddy_quota_alert_enabled),
            workbuddy_quota_alert_threshold: Some(current.workbuddy_quota_alert_threshold),
        }
    }

    #[test]
    fn openclaw_auth_overwrite_default_is_disabled() {
        let cfg = UserConfig::default();
        assert!(!cfg.openclaw_auth_overwrite_on_switch);
    }

    #[test]
    fn openclaw_auth_overwrite_missing_field_falls_back_to_disabled() {
        let cfg: UserConfig =
            serde_json::from_value(serde_json::json!({})).expect("反序列化默认配置应成功");
        assert!(!cfg.openclaw_auth_overwrite_on_switch);
    }

    #[test]
    fn network_config_update_preserves_optional_values() {
        let mut current = UserConfig::default();
        current.report_enabled = true;
        current.report_port = 18081;
        current.report_token = "existing-token".to_string();
        current.global_proxy_enabled = false;
        current.global_proxy_url = "http://127.0.0.1:7890".to_string();
        current.global_proxy_no_proxy = "localhost".to_string();

        let update = NetworkConfigUpdate {
            ws_enabled: current.ws_enabled,
            ws_port: current.ws_port,
            global_proxy_no_proxy: Some("  localhost,127.0.0.1  ".to_string()),
            ..NetworkConfigUpdate::default()
        };

        let (next, needs_restart) =
            resolve_network_config_update(current, update).expect("update should be valid");

        assert!(!needs_restart);
        assert_eq!(next.report_token, "existing-token");
        assert_eq!(next.global_proxy_url, "http://127.0.0.1:7890");
        assert_eq!(next.global_proxy_no_proxy, "localhost,127.0.0.1");
    }

    #[test]
    fn network_config_update_detects_restart_fields() {
        let current = UserConfig::default();
        let update = NetworkConfigUpdate {
            ws_enabled: !current.ws_enabled,
            ws_port: current.ws_port + 1,
            report_token: Some("new-token".to_string()),
            ..NetworkConfigUpdate::default()
        };

        let (_next, needs_restart) =
            resolve_network_config_update(current, update).expect("update should be valid");

        assert!(needs_restart);
    }

    #[test]
    fn network_config_update_rejects_empty_report_token_when_enabled() {
        let current = UserConfig::default();
        let update = NetworkConfigUpdate {
            ws_enabled: current.ws_enabled,
            ws_port: current.ws_port,
            report_enabled: Some(true),
            report_token: Some("   ".to_string()),
            ..NetworkConfigUpdate::default()
        };

        assert!(resolve_network_config_update(current, update).is_err());
    }

    #[test]
    fn network_config_update_rejects_empty_proxy_url_when_enabled() {
        let current = UserConfig::default();
        let update = NetworkConfigUpdate {
            ws_enabled: current.ws_enabled,
            ws_port: current.ws_port,
            global_proxy_enabled: Some(true),
            global_proxy_url: Some("   ".to_string()),
            ..NetworkConfigUpdate::default()
        };

        assert!(resolve_network_config_update(current, update).is_err());
    }

    #[test]
    fn general_config_projection_matches_frontend_shape() {
        let mut current = UserConfig::default();
        current.close_behavior = CloseWindowBehavior::Quit;
        current.minimize_behavior = MinimizeWindowBehavior::TrayOnly;
        current.tray_icon_style = TrayIconStyle::Color;
        current.app_auto_launch_enabled = false;
        current.antigravity_startup_wakeup_delay_seconds = -1;
        current.codex_startup_wakeup_delay_seconds = 24 * 60 * 60 + 1;

        let config = general_config_from_user_config(current, Some(true));

        assert_eq!(config.close_behavior, "quit");
        assert_eq!(config.minimize_behavior, "tray_only");
        assert_eq!(config.tray_icon_style, "color");
        assert!(config.app_auto_launch_enabled);
        assert_eq!(config.antigravity_startup_wakeup_delay_seconds, 0);
        assert_eq!(config.codex_startup_wakeup_delay_seconds, 24 * 60 * 60);
    }

    #[test]
    fn general_config_save_preserves_desktop_optional_semantics() {
        let mut current = UserConfig::default();
        current.default_terminal = "custom terminal".to_string();
        current.ws_enabled = false;
        current.ws_port = 19999;
        current.auto_backup_enabled = false;
        current.auto_switch_scope_mode = "selected_groups".to_string();
        current.opencode_auth_overwrite_on_switch = true;
        current.opencode_sync_on_switch = true;
        current.codex_quota_alert_threshold = 80;
        current.codex_quota_alert_primary_threshold = 70;
        current.codex_quota_alert_secondary_threshold = 60;

        let mut input = default_general_save_input(&current);
        input.language = "EN".to_string();
        input.default_terminal = None;
        input.ui_scale = Some(9.0);
        input.codex_wsl_config_dir = Some("  /wsl/codex  ".to_string());
        input.opencode_app_path = "  /apps/opencode  ".to_string();
        input.opencode_auth_overwrite_on_switch = Some(false);
        input.opencode_sync_on_switch = true;
        input.auto_switch_scope_mode = Some("   ".to_string());
        input.auto_switch_account_scope_mode = Some("selected_accounts".to_string());
        input.auto_switch_selected_account_ids = Some(vec![
            "  a  ".to_string(),
            String::new(),
            "a".to_string(),
            "b".to_string(),
        ]);
        input.codex_quota_alert_threshold = Some(35);
        input.codex_quota_alert_primary_threshold = None;
        input.codex_quota_alert_secondary_threshold = None;

        let next = resolve_general_config_save(current, input);

        assert_eq!(next.language, "en");
        assert_eq!(next.default_terminal, "custom terminal");
        assert_eq!(next.ui_scale, 2.0);
        assert_eq!(next.codex_wsl_config_dir, "/wsl/codex");
        assert_eq!(next.opencode_app_path, "/apps/opencode");
        assert!(!next.opencode_auth_overwrite_on_switch);
        assert!(!next.opencode_sync_on_switch);
        assert_eq!(next.auto_switch_scope_mode, "selected_groups");
        assert_eq!(next.auto_switch_account_scope_mode, "selected_accounts");
        assert_eq!(next.auto_switch_selected_account_ids, vec!["a", "b"]);
        assert_eq!(next.codex_quota_alert_threshold, 35);
        assert_eq!(next.codex_quota_alert_primary_threshold, 35);
        assert_eq!(next.codex_quota_alert_secondary_threshold, 35);
        assert!(!next.ws_enabled);
        assert_eq!(next.ws_port, 19999);
        assert!(!next.auto_backup_enabled);
    }

    #[test]
    fn general_config_patch_merges_camel_case_without_touching_network() {
        let mut current = UserConfig::default();
        current.language = "zh-cn".to_string();
        current.theme = "dark".to_string();
        current.ws_enabled = false;
        current.ws_port = 19999;
        current.report_token = "network-token".to_string();
        current.opencode_auth_overwrite_on_switch = true;
        current.opencode_sync_on_switch = true;
        current.codex_quota_alert_threshold = 80;
        current.codex_quota_alert_primary_threshold = 70;
        current.codex_quota_alert_secondary_threshold = 60;

        let next = super::resolve_general_config_patch(
            current,
            &serde_json::json!({
                "language": "EN",
                "uiScale": 9.0,
                "opencodeAppPath": "  /tmp/opencode  ",
                "opencodeAuthOverwriteOnSwitch": false,
                "opencodeSyncOnSwitch": true,
                "autoSwitchAccountScopeMode": "selected_accounts",
                "autoSwitchSelectedAccountIds": ["  a  ", "", "a", "b"],
                "codexQuotaAlertThreshold": 42,
                "wsEnabled": true,
                "wsPort": 18000
            }),
        )
        .expect("general config patch should be valid");

        assert_eq!(next.language, "en");
        assert_eq!(next.theme, "dark");
        assert_eq!(next.ui_scale, 2.0);
        assert_eq!(next.opencode_app_path, "/tmp/opencode");
        assert!(!next.opencode_auth_overwrite_on_switch);
        assert!(!next.opencode_sync_on_switch);
        assert_eq!(next.auto_switch_account_scope_mode, "selected_accounts");
        assert_eq!(next.auto_switch_selected_account_ids, vec!["a", "b"]);
        assert_eq!(next.codex_quota_alert_threshold, 42);
        assert_eq!(next.codex_quota_alert_primary_threshold, 42);
        assert_eq!(next.codex_quota_alert_secondary_threshold, 42);
        assert!(!next.ws_enabled);
        assert_eq!(next.ws_port, 19999);
        assert_eq!(next.report_token, "network-token");
    }

    #[test]
    fn set_app_path_updates_single_normalized_path() {
        let mut current = UserConfig::default();
        current.codex_app_path = "/old/codex".to_string();
        current.windsurf_app_path = "/old/windsurf".to_string();
        current.ws_port = 19999;

        let next = resolve_set_app_path(current, "codex", "  /new/codex  ")
            .expect("codex path should be supported");

        assert_eq!(next.codex_app_path, "/new/codex");
        assert_eq!(next.windsurf_app_path, "/old/windsurf");
        assert_eq!(next.ws_port, 19999);
    }

    #[test]
    fn close_behavior_from_window_action_matches_desktop_compatibility() {
        assert_eq!(
            close_behavior_from_window_action("minimize"),
            CloseWindowBehavior::Minimize
        );
        assert_eq!(
            close_behavior_from_window_action("quit"),
            CloseWindowBehavior::Quit
        );
        assert_eq!(
            close_behavior_from_window_action("unknown"),
            CloseWindowBehavior::Ask
        );
    }

    #[test]
    fn close_behavior_resolver_preserves_invalid_action_as_ask() {
        let mut current = UserConfig::default();
        current.close_behavior = CloseWindowBehavior::Quit;
        current.language = "en".to_string();

        let (next, changed) = resolve_save_close_behavior_for_window_action(current, "unknown");

        assert!(changed);
        assert_eq!(next.close_behavior, CloseWindowBehavior::Ask);
        assert_eq!(next.language, "en");
    }

    #[test]
    fn small_config_mutators_only_update_target_fields() {
        let mut current = UserConfig::default();
        current.codex_launch_on_switch = false;
        current.codex_local_access_entry_visible = true;
        current.floating_card_always_on_top = false;
        current.floating_card_confirm_on_close = true;
        current.floating_card_position_x = Some(10);
        current.floating_card_position_y = Some(20);
        current.ws_port = 19999;

        let (current, changed) = resolve_set_codex_launch_on_switch(current, true);
        assert!(changed);
        assert!(current.codex_launch_on_switch);
        assert_eq!(current.ws_port, 19999);

        let (current, changed) = resolve_set_codex_local_access_entry_visible(current, false);
        assert!(changed);
        assert!(!current.codex_local_access_entry_visible);
        assert_eq!(current.ws_port, 19999);

        let (current, changed) = resolve_set_floating_card_always_on_top(current, true);
        assert!(changed);
        assert!(current.floating_card_always_on_top);
        assert_eq!(current.ws_port, 19999);

        let (current, changed) = resolve_set_floating_card_confirm_on_close(current, false);
        assert!(changed);
        assert!(!current.floating_card_confirm_on_close);
        assert_eq!(current.ws_port, 19999);

        let (current, changed) = resolve_save_floating_card_position(current, 30, 40);
        assert!(changed);
        assert_eq!(current.floating_card_position_x, Some(30));
        assert_eq!(current.floating_card_position_y, Some(40));
        assert_eq!(current.ws_port, 19999);
    }

    #[test]
    fn small_config_mutators_report_noop_when_unchanged() {
        let mut current = UserConfig::default();
        current.codex_launch_on_switch = true;
        current.codex_local_access_entry_visible = false;
        current.floating_card_always_on_top = true;
        current.floating_card_confirm_on_close = false;
        current.floating_card_position_x = Some(30);
        current.floating_card_position_y = Some(40);

        assert!(!resolve_set_codex_launch_on_switch(current.clone(), true).1);
        assert!(!resolve_set_codex_local_access_entry_visible(current.clone(), false).1);
        assert!(!resolve_set_floating_card_always_on_top(current.clone(), true).1);
        assert!(!resolve_set_floating_card_confirm_on_close(current.clone(), false).1);
        assert!(!resolve_save_floating_card_position(current, 30, 40).1);
    }

    #[test]
    fn set_app_path_rejects_unknown_app() {
        let err = resolve_set_app_path(UserConfig::default(), "unknown", "/app")
            .expect_err("unknown app should be rejected");

        assert_eq!(err, "未知应用类型");
    }
}
