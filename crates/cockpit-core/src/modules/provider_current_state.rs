use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const PROVIDER_CURRENT_STATE_FILE: &str = "provider_current_accounts.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderCurrentState {
    #[serde(default = "default_version")]
    version: String,
    #[serde(default)]
    current_accounts: HashMap<String, String>,
}

fn default_version() -> String {
    "1.0".to_string()
}

impl ProviderCurrentState {
    fn new() -> Self {
        Self {
            version: default_version(),
            current_accounts: HashMap::new(),
        }
    }
}

fn normalize_platform(platform: &str) -> Result<&'static str, String> {
    match platform.trim() {
        "windsurf" => Ok("windsurf"),
        "kiro" => Ok("kiro"),
        "cursor" => Ok("cursor"),
        "gemini" => Ok("gemini"),
        "codebuddy" => Ok("codebuddy"),
        "codebuddy_cn" | "codebuddy-cn" => Ok("codebuddy_cn"),
        "qoder" => Ok("qoder"),
        "trae" => Ok("trae"),
        "workbuddy" => Ok("workbuddy"),
        "github_copilot" | "github-copilot" | "ghcp" => Ok("github_copilot"),
        "zed" => Ok("zed"),
        other => Err(format!("unsupported platform: {other}")),
    }
}

pub fn normalize_provider_platform(platform: &str) -> Result<&'static str, String> {
    normalize_platform(platform)
}

pub fn resolve_provider_current_account_id(platform: &str) -> Result<Option<String>, String> {
    match normalize_platform(platform)? {
        "windsurf" => {
            let accounts = crate::modules::windsurf_account::list_accounts();
            Ok(crate::modules::windsurf_account::resolve_current_account_id(&accounts))
        }
        "kiro" => {
            let accounts = crate::modules::kiro_account::list_accounts();
            Ok(crate::modules::kiro_account::resolve_current_account_id(
                &accounts,
            ))
        }
        "cursor" => {
            let accounts = crate::modules::cursor_account::list_accounts();
            Ok(crate::modules::cursor_account::resolve_current_account_id(
                &accounts,
            ))
        }
        "gemini" => {
            let accounts = crate::modules::gemini_account::list_accounts();
            Ok(
                crate::modules::gemini_account::resolve_current_account(&accounts)
                    .map(|account| account.id),
            )
        }
        "codebuddy" => {
            let accounts = crate::modules::codebuddy_account::list_accounts();
            Ok(crate::modules::codebuddy_account::resolve_current_account_id(&accounts))
        }
        "codebuddy_cn" => {
            let accounts = crate::modules::codebuddy_cn_account::list_accounts();
            Ok(crate::modules::codebuddy_cn_account::resolve_current_account_id(&accounts))
        }
        "qoder" => {
            let accounts = crate::modules::qoder_account::list_accounts();
            Ok(crate::modules::qoder_account::resolve_current_account_id(
                &accounts,
            ))
        }
        "trae" => {
            let accounts = crate::modules::trae_account::list_accounts();
            Ok(crate::modules::trae_account::resolve_current_account_id(
                &accounts,
            ))
        }
        "workbuddy" => {
            let accounts = crate::modules::workbuddy_account::list_accounts();
            Ok(crate::modules::workbuddy_account::resolve_current_account_id(&accounts))
        }
        "github_copilot" => {
            let accounts = crate::modules::github_copilot_account::list_accounts();
            Ok(crate::modules::github_copilot_account::resolve_current_account_id(&accounts))
        }
        "zed" => Ok(crate::modules::zed_account::resolve_current_account_id()),
        other => Err(format!("unsupported platform: {other}")),
    }
}

fn state_path() -> Result<PathBuf, String> {
    Ok(crate::modules::account::get_data_dir()?.join(PROVIDER_CURRENT_STATE_FILE))
}

fn load_state() -> Result<ProviderCurrentState, String> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(ProviderCurrentState::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|err| format!("read provider current state failed: {err}"))?;
    if content.trim().is_empty() {
        return Ok(ProviderCurrentState::new());
    }

    crate::modules::atomic_write::parse_json_with_auto_restore::<ProviderCurrentState>(
        &path, &content,
    )
    .map_err(|err| format!("parse provider current state failed: {err}"))
}

fn save_state(state: &ProviderCurrentState) -> Result<(), String> {
    let path = state_path()?;
    let content = serde_json::to_string_pretty(state)
        .map_err(|err| format!("serialize provider current state failed: {err}"))?;
    crate::modules::atomic_write::write_string_atomic(&path, &content)
        .map_err(|err| format!("write provider current state failed: {err}"))
}

pub fn get_current_account_id(platform: &str) -> Result<Option<String>, String> {
    let key = normalize_platform(platform)?;
    let state = load_state()?;
    Ok(state
        .current_accounts
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

pub fn resolve_existing_current_account_id<'a, I>(platform: &str, existing_ids: I) -> Option<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let current_id = get_current_account_id(platform).ok().flatten()?;
    if existing_ids.into_iter().any(|id| id == current_id.as_str()) {
        Some(current_id)
    } else {
        let _ = set_current_account_id(platform, None);
        None
    }
}

pub fn set_current_account_id(platform: &str, account_id: Option<&str>) -> Result<(), String> {
    let key = normalize_platform(platform)?;
    let mut state = load_state()?;
    if let Some(account_id) = account_id.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    }) {
        state
            .current_accounts
            .insert(key.to_string(), account_id.to_string());
    } else {
        state.current_accounts.remove(key);
    }
    save_state(&state)
}

#[cfg(test)]
mod tests {
    use super::normalize_provider_platform;

    #[test]
    fn normalizes_provider_platform_aliases() {
        assert_eq!(
            normalize_provider_platform("github-copilot").unwrap(),
            "github_copilot"
        );
        assert_eq!(
            normalize_provider_platform("github_copilot").unwrap(),
            "github_copilot"
        );
        assert_eq!(
            normalize_provider_platform("ghcp").unwrap(),
            "github_copilot"
        );
        assert_eq!(
            normalize_provider_platform("codebuddy-cn").unwrap(),
            "codebuddy_cn"
        );
        assert_eq!(
            normalize_provider_platform("codebuddy_cn").unwrap(),
            "codebuddy_cn"
        );
        assert_eq!(normalize_provider_platform(" zed ").unwrap(), "zed");
        assert!(normalize_provider_platform("unknown").is_err());
    }
}
