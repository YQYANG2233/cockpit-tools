use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::config::get_shared_dir;

const GROUP_SETTINGS_FILE: &str = "group_settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    Plugin,
    Desktop,
}

impl Default for ConfigSource {
    fn default() -> Self {
        Self::Desktop
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroupSettings {
    #[serde(default)]
    pub group_mappings: HashMap<String, String>,
    #[serde(default)]
    pub group_names: HashMap<String, String>,
    #[serde(default)]
    pub group_order: Vec<String>,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub updated_by: ConfigSource,
}

impl Default for GroupSettings {
    fn default() -> Self {
        let mut group_mappings = HashMap::new();
        let mut group_names = HashMap::new();

        group_mappings.insert(
            "claude-opus-4-5-thinking".to_string(),
            "claude_45".to_string(),
        );
        group_mappings.insert(
            "claude-opus-4-6-thinking".to_string(),
            "claude_45".to_string(),
        );
        group_mappings.insert("claude-sonnet-4-5".to_string(), "claude_45".to_string());
        group_mappings.insert("claude-sonnet-4-6".to_string(), "claude_45".to_string());
        group_mappings.insert(
            "claude-sonnet-4-5-thinking".to_string(),
            "claude_45".to_string(),
        );
        group_mappings.insert("gpt-oss-120b-medium".to_string(), "claude_45".to_string());
        group_names.insert("claude_45".to_string(), "Claude 4.5".to_string());

        group_mappings.insert("gemini-3-pro-high".to_string(), "g3_pro".to_string());
        group_mappings.insert("gemini-3-pro-low".to_string(), "g3_pro".to_string());
        group_mappings.insert("gemini-3.1-pro-high".to_string(), "g3_pro".to_string());
        group_mappings.insert("gemini-3.1-pro-low".to_string(), "g3_pro".to_string());
        group_names.insert("g3_pro".to_string(), "Gemini Pro".to_string());

        group_mappings.insert("gemini-3-flash".to_string(), "g3_flash".to_string());
        group_names.insert("g3_flash".to_string(), "Gemini Flash".to_string());

        Self {
            group_mappings,
            group_names,
            group_order: vec![
                "claude_45".to_string(),
                "g3_pro".to_string(),
                "g3_flash".to_string(),
            ],
            updated_at: 0,
            updated_by: ConfigSource::Desktop,
        }
    }
}

impl GroupSettings {
    pub fn set_model_group(&mut self, model_id: &str, group_id: &str) {
        self.group_mappings
            .insert(model_id.to_string(), group_id.to_string());
        self.touch();
    }

    pub fn remove_model_group(&mut self, model_id: &str) {
        self.group_mappings.remove(model_id);
        self.touch();
    }

    pub fn set_group_name(&mut self, group_id: &str, name: &str) {
        self.group_names
            .insert(group_id.to_string(), name.to_string());
        self.touch();
    }

    pub fn delete_group(&mut self, group_id: &str) {
        self.group_mappings.retain(|_, value| value != group_id);
        self.group_names.remove(group_id);
        self.group_order.retain(|value| value != group_id);
        self.touch();
    }

    pub fn set_group_order(&mut self, order: Vec<String>) {
        self.group_order = order;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self.updated_by = ConfigSource::Desktop;
    }
}

fn group_settings_path() -> PathBuf {
    get_shared_dir().join(GROUP_SETTINGS_FILE)
}

fn merge_default_settings(settings: &mut GroupSettings) {
    let default_settings = GroupSettings::default();
    for (model_id, group_id) in default_settings.group_mappings {
        settings.group_mappings.entry(model_id).or_insert(group_id);
    }
    for (group_id, name) in default_settings.group_names {
        settings.group_names.entry(group_id).or_insert(name);
    }
    if settings.group_order.is_empty() {
        settings.group_order = default_settings.group_order;
    } else {
        for group_id in default_settings.group_order {
            if !settings.group_order.contains(&group_id) {
                settings.group_order.push(group_id);
            }
        }
    }
    settings.group_names.remove("g3_image");
    settings
        .group_order
        .retain(|group_id| group_id != "g3_image");
    settings
        .group_mappings
        .retain(|model_id, group_id| group_id != "g3_image" && model_id != "gemini-3-pro-image");
}

pub fn load_group_settings() -> GroupSettings {
    let path = group_settings_path();
    if !path.exists() {
        return GroupSettings::default();
    }

    let Ok(content) = fs::read_to_string(&path) else {
        return GroupSettings::default();
    };

    if content.trim().is_empty() {
        return GroupSettings::default();
    }

    match crate::modules::atomic_write::parse_json_with_auto_restore::<GroupSettings>(
        &path, &content,
    ) {
        Ok(mut settings) => {
            let original = settings.clone();
            merge_default_settings(&mut settings);
            if settings != original {
                let _ = save_group_settings(&settings);
            }
            settings
        }
        Err(err) => {
            crate::modules::logger::log_warn(&format!(
                "[GroupSettings] failed to parse {}, using defaults: {}",
                path.display(),
                err
            ));
            GroupSettings::default()
        }
    }
}

pub fn save_group_settings(settings: &GroupSettings) -> Result<(), String> {
    let path = group_settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create group settings dir failed: {err}"))?;
    }
    let content = serde_json::to_string_pretty(settings)
        .map_err(|err| format!("serialize group settings failed: {err}"))?;
    crate::modules::atomic_write::write_string_atomic(&path, &content)
        .map_err(|err| format!("write group settings failed: {err}"))
}

pub fn update_group_settings(settings: GroupSettings) -> Result<(), String> {
    save_group_settings(&settings)
}
