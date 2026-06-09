use chrono::{DateTime, Duration as ChronoDuration, Utc};
use reqwest_dav::types::list_cmd::ListEntity;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::modules::config::UserConfig;

#[derive(Debug, Clone)]
pub struct WebdavConnectionSettings {
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub remote_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebdavBackupFileEntry {
    pub file_name: String,
    pub file_kind: String,
    pub size_bytes: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebdavTestResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebdavUploadResult {
    pub uploaded_files: Vec<WebdavBackupFileEntry>,
    pub deleted_files: Vec<String>,
    pub uploaded_at: String,
    pub remote_dir: String,
}

pub struct WebdavSyncClient {
    pub client: reqwest_dav::Client,
    pub remote_dir: String,
}

impl WebdavSyncClient {
    pub fn new(settings: &WebdavConnectionSettings) -> Result<Self, String> {
        let auth = reqwest_dav::Auth::Basic(settings.username.clone(), settings.password.clone());
        let client = reqwest_dav::ClientBuilder::new()
            .set_host(settings.base_url.clone())
            .set_auth(auth)
            .build()
            .map_err(|err| format!("create WebDAV client failed: {err:?}"))?;

        Ok(Self {
            client,
            remote_dir: settings.remote_dir.clone(),
        })
    }

    pub async fn check_dir_exists(&self, path: &str) -> bool {
        match self.client.list(path, reqwest_dav::Depth::Number(0)).await {
            Ok(_) => true,
            Err(reqwest_dav::Error::Decode(reqwest_dav::DecodeError::StatusMismatched(err))) => {
                err.response_code != 404
            }
            Err(_) => false,
        }
    }

    pub async fn ensure_remote_dir(&self) -> Result<(), String> {
        let mut current_dir = String::new();
        for part in self.remote_dir.split('/') {
            if part.is_empty() {
                continue;
            }
            if !current_dir.is_empty() {
                current_dir.push('/');
            }
            current_dir.push_str(part);
            if self.check_dir_exists(&current_dir).await {
                continue;
            }
            if let Err(err) = self.client.mkcol(&current_dir).await {
                if let reqwest_dav::Error::Decode(reqwest_dav::DecodeError::StatusMismatched(
                    status_err,
                )) = &err
                {
                    if status_err.response_code == 405 {
                        continue;
                    }
                }
                return Err(format!("create WebDAV remote directory failed: {err:?}"));
            }
        }
        Ok(())
    }

    pub async fn list_remote_backups(&self) -> Result<Vec<WebdavBackupFileEntry>, String> {
        let mut files = Vec::new();

        if !self.check_dir_exists(&self.remote_dir).await {
            return Ok(files);
        }

        let entities = self
            .client
            .list(&self.remote_dir, reqwest_dav::Depth::Number(1))
            .await
            .map_err(|err| format!("read WebDAV backup list failed: {err:?}"))?;

        for entity in entities {
            let ListEntity::File(file) = entity else {
                continue;
            };
            let Some(raw_name) = file.href.rsplit('/').find(|value| !value.is_empty()) else {
                continue;
            };
            let file_name = match urlencoding::decode(raw_name) {
                Ok(decoded) => decoded.to_string(),
                Err(err) => {
                    tracing::warn!("skip undecodable WebDAV file name [{raw_name}]: {err}");
                    continue;
                }
            };

            if !is_backup_file_name(&file_name) {
                continue;
            }

            files.push(WebdavBackupFileEntry {
                file_kind: file_kind(&file_name).to_string(),
                file_name,
                size_bytes: file.content_length as u64,
                modified_at: Some(file.last_modified.to_rfc3339()),
            });
        }

        files.sort_by(|left, right| {
            modified_sort_key(right)
                .cmp(&modified_sort_key(left))
                .then_with(|| right.file_name.cmp(&left.file_name))
        });

        Ok(files)
    }

    pub async fn upload_backup_bytes(
        &self,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<WebdavBackupFileEntry, String> {
        if !is_backup_file_name(file_name) {
            return Err("WebDAV only allows Cockpit backup files".to_string());
        }
        self.ensure_remote_dir().await?;

        let path = format!("{}/{}", self.remote_dir, file_name);
        let size_bytes = bytes.len() as u64;

        self.client
            .put(&path, bytes)
            .await
            .map_err(|err| format!("upload WebDAV backup failed: {err:?}"))?;

        Ok(WebdavBackupFileEntry {
            file_name: file_name.to_string(),
            file_kind: file_kind(file_name).to_string(),
            size_bytes,
            modified_at: Some(Utc::now().to_rfc3339()),
        })
    }

    pub async fn read_remote_backup(&self, file_name: &str) -> Result<String, String> {
        if !is_backup_file_name(file_name) || !file_name.ends_with(".json") {
            return Err("only JSON backup files can be restored directly from WebDAV".to_string());
        }
        let path = format!("{}/{}", self.remote_dir, file_name);
        let response = self
            .client
            .get(&path)
            .await
            .map_err(|err| format!("read WebDAV backup failed: {err:?}"))?;

        response
            .text()
            .await
            .map_err(|err| format!("read WebDAV backup content failed: {err}"))
    }

    pub async fn read_remote_backup_bytes(&self, file_name: &str) -> Result<Vec<u8>, String> {
        if !is_backup_file_name(file_name) {
            return Err("WebDAV only allows reading Cockpit backup files".to_string());
        }
        let path = format!("{}/{}", self.remote_dir, file_name);
        let response = self
            .client
            .get(&path)
            .await
            .map_err(|err| format!("read WebDAV backup failed: {err:?}"))?;

        let bytes = response
            .bytes()
            .await
            .map_err(|err| format!("read WebDAV backup content failed: {err}"))?;
        Ok(bytes.to_vec())
    }

    pub async fn delete_remote_backup(&self, file_name: &str) -> Result<(), String> {
        if !is_backup_file_name(file_name) {
            return Err("WebDAV only allows deleting Cockpit backup files".to_string());
        }
        let path = format!("{}/{}", self.remote_dir, file_name);

        if let Err(err) = self.client.delete(&path).await {
            if let reqwest_dav::Error::Decode(reqwest_dav::DecodeError::StatusMismatched(
                status_err,
            )) = &err
            {
                if status_err.response_code == 404 {
                    return Ok(());
                }
            }
            return Err(format!("delete WebDAV backup failed: {err:?}"));
        }
        Ok(())
    }

    pub async fn cleanup_remote_backups(&self, retention_days: i32) -> Result<Vec<String>, String> {
        let mut deleted = Vec::new();

        if !self.check_dir_exists(&self.remote_dir).await {
            return Ok(deleted);
        }

        let entities = self
            .client
            .list(&self.remote_dir, reqwest_dav::Depth::Number(1))
            .await
            .map_err(|err| format!("read WebDAV backup list failed: {err:?}"))?;

        let cutoff = Utc::now() - ChronoDuration::days(retention_days.max(1) as i64);

        for entity in entities {
            let ListEntity::File(file) = entity else {
                continue;
            };
            let Some(raw_name) = file.href.rsplit('/').find(|value| !value.is_empty()) else {
                continue;
            };
            let file_name = match urlencoding::decode(raw_name) {
                Ok(decoded) => decoded.to_string(),
                Err(err) => {
                    tracing::warn!("skip undecodable WebDAV file name [{raw_name}]: {err}");
                    continue;
                }
            };

            if !is_backup_file_name(&file_name) || file.last_modified >= cutoff {
                continue;
            }

            let path = format!("{}/{}", self.remote_dir, file_name);
            if let Err(err) = self.client.delete(&path).await {
                if let reqwest_dav::Error::Decode(reqwest_dav::DecodeError::StatusMismatched(
                    status_err,
                )) = &err
                {
                    if status_err.response_code == 404 {
                        continue;
                    }
                }
                tracing::error!("delete expired WebDAV backup [{file_name}] failed: {err:?}");
                continue;
            }
            deleted.push(file_name);
        }

        deleted.sort();
        Ok(deleted)
    }
}

pub fn normalize_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("WebDAV address cannot be empty".to_string());
    }

    let mut url = Url::parse(trimmed).map_err(|err| format!("invalid WebDAV address: {err}"))?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("WebDAV address must start with http or https".to_string()),
    }
    url.set_query(None);
    url.set_fragment(None);

    let mut value = url.to_string();
    if !value.ends_with('/') {
        value.push('/');
    }
    Ok(value)
}

pub fn normalize_remote_dir(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err("WebDAV remote directory cannot be empty".to_string());
    }
    if trimmed.contains('\\') {
        return Err("WebDAV remote directory cannot contain backslashes".to_string());
    }

    let mut parts = Vec::new();
    for part in trimmed.split('/') {
        let normalized = part.trim();
        if normalized.is_empty() {
            return Err("WebDAV remote directory cannot contain empty path segments".to_string());
        }
        let decoded = urlencoding::decode(normalized)
            .map_err(|err| format!("invalid WebDAV remote directory encoding: {err}"))?;
        if decoded == "." || decoded == ".." || decoded.contains('\\') {
            return Err("WebDAV remote directory cannot contain traversal segments".to_string());
        }
        parts.push(normalized.to_string());
    }

    Ok(parts.join("/"))
}

pub fn is_backup_file_name(file_name: &str) -> bool {
    let trimmed = file_name.trim();
    if trimmed != file_name || trimmed.contains('/') || trimmed.contains('\\') {
        return false;
    }
    let matches_prefix = trimmed.starts_with("cockpit_auto_backup_")
        || trimmed.starts_with("cockpit_manual_backup_");
    let matches_suffix = trimmed.ends_with(".json") || trimmed.ends_with(".zip");
    matches_prefix && matches_suffix
}

pub fn connection_from_config(config: &UserConfig) -> Result<WebdavConnectionSettings, String> {
    connection_from_parts(
        &config.webdav_sync_url,
        &config.webdav_sync_username,
        &config.webdav_sync_password,
        &config.webdav_sync_remote_dir,
    )
}

pub fn connection_from_parts(
    base_url: &str,
    username: &str,
    password: &str,
    remote_dir: &str,
) -> Result<WebdavConnectionSettings, String> {
    let normalized_base_url = normalize_base_url(base_url)?;
    let normalized_remote_dir = normalize_remote_dir(remote_dir)?;
    let normalized_username = username.trim().to_string();
    if normalized_username.is_empty() {
        return Err("WebDAV username cannot be empty".to_string());
    }
    if password.is_empty() {
        return Err("WebDAV app password cannot be empty".to_string());
    }

    Ok(WebdavConnectionSettings {
        base_url: normalized_base_url,
        username: normalized_username,
        password: password.to_string(),
        remote_dir: normalized_remote_dir,
    })
}

pub async fn test_connection(
    settings: &WebdavConnectionSettings,
) -> Result<WebdavTestResult, String> {
    let client = WebdavSyncClient::new(settings)?;
    client.ensure_remote_dir().await?;
    let _ = client
        .client
        .list(&settings.remote_dir, reqwest_dav::Depth::Number(1))
        .await
        .map_err(|err| format!("WebDAV connection test failed: {err:?}"))?;
    Ok(WebdavTestResult {
        ok: true,
        message: "WebDAV connection succeeded".to_string(),
    })
}

pub async fn list_remote_backups(
    settings: &WebdavConnectionSettings,
) -> Result<Vec<WebdavBackupFileEntry>, String> {
    let client = WebdavSyncClient::new(settings)?;
    client.list_remote_backups().await
}

pub async fn read_remote_backup(
    settings: &WebdavConnectionSettings,
    file_name: &str,
) -> Result<String, String> {
    let client = WebdavSyncClient::new(settings)?;
    client.read_remote_backup(file_name).await
}

pub async fn read_remote_backup_bytes(
    settings: &WebdavConnectionSettings,
    file_name: &str,
) -> Result<Vec<u8>, String> {
    let client = WebdavSyncClient::new(settings)?;
    client.read_remote_backup_bytes(file_name).await
}

pub async fn delete_remote_backup(
    settings: &WebdavConnectionSettings,
    file_name: &str,
) -> Result<(), String> {
    let client = WebdavSyncClient::new(settings)?;
    client.delete_remote_backup(file_name).await
}

fn modified_sort_key(file: &WebdavBackupFileEntry) -> i64 {
    file.modified_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp())
        .unwrap_or_default()
}

fn file_kind(file_name: &str) -> &str {
    if file_name.ends_with(".zip") {
        "zip"
    } else {
        "json"
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_base_url, normalize_remote_dir};

    #[test]
    fn normalize_webdav_target_rejects_invalid_values() {
        assert!(normalize_base_url("").is_err());
        assert!(normalize_base_url("ftp://dav.example.com/dav/").is_err());
        assert!(normalize_remote_dir("../backups").is_err());
        assert!(normalize_remote_dir("CockpitTools\\backups").is_err());
    }

    #[test]
    fn normalize_webdav_target_trims_valid_values() {
        assert_eq!(
            normalize_base_url(" https://dav.jianguoyun.com/dav/ ").unwrap(),
            "https://dav.jianguoyun.com/dav/"
        );
        assert_eq!(
            normalize_remote_dir(" /cockpit-tools/ ").unwrap(),
            "cockpit-tools"
        );
    }
}
