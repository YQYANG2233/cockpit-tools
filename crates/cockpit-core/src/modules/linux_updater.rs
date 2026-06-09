use serde::{Deserialize, Serialize};

pub const UPDATE_PROGRESS_EVENT: &str = "update://linux-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateRuntimeInfo {
    pub platform: String,
    pub linux_install_kind: String,
    pub linux_managed_install_supported: bool,
    pub updater_target: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdaterPluginConfig {
    pub pubkey: String,
    pub endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinuxUpdateProgressPayload {
    pub version: String,
    pub phase: String,
    pub progress: Option<u8>,
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{
        LinuxUpdateProgressPayload, UpdateRuntimeInfo, UpdaterPluginConfig, UPDATE_PROGRESS_EVENT,
    };
    use crate::modules::logger;
    use base64::Engine;
    use futures_util::StreamExt;
    use minisign_verify::{PublicKey, Signature};
    use serde::Deserialize;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use tokio::fs;
    use tokio::io::AsyncWriteExt;
    use url::Url;

    const ZH_SECTION_HEADER: &str = "## 更新日志（中文）";
    const EN_SECTION_HEADER: &str = "## Changelog (English)";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum LinuxInstallKind {
        Deb,
        Rpm,
        AppImage,
        Unknown,
    }

    impl LinuxInstallKind {
        fn as_str(self) -> &'static str {
            match self {
                Self::Deb => "deb",
                Self::Rpm => "rpm",
                Self::AppImage => "appimage",
                Self::Unknown => "unknown",
            }
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    struct LatestManifest {
        version: String,
        #[serde(default)]
        notes: String,
        platforms: std::collections::HashMap<String, LatestPlatform>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct LatestPlatform {
        signature: String,
        url: String,
    }

    pub fn get_update_runtime_info() -> UpdateRuntimeInfo {
        let install_kind = detect_linux_install_kind();
        UpdateRuntimeInfo {
            platform: "linux".to_string(),
            linux_install_kind: install_kind.as_str().to_string(),
            linux_managed_install_supported: matches!(
                install_kind,
                LinuxInstallKind::Deb | LinuxInstallKind::Rpm
            ),
            updater_target: None,
        }
    }

    pub async fn install_linux_update<SaveNotes, EmitProgress>(
        config: UpdaterPluginConfig,
        current_version: &str,
        expected_version: Option<String>,
        save_pending_notes: SaveNotes,
        emit_progress: EmitProgress,
    ) -> Result<(), String>
    where
        SaveNotes: Fn(String, String, String) -> Result<(), String>,
        EmitProgress: Fn(LinuxUpdateProgressPayload),
    {
        let install_kind = detect_linux_install_kind();
        if !matches!(install_kind, LinuxInstallKind::Deb | LinuxInstallKind::Rpm) {
            return Err(format!(
                "Current Linux install kind '{}' does not support managed package update",
                install_kind.as_str()
            ));
        }

        let endpoint = config
            .endpoints
            .first()
            .cloned()
            .ok_or_else(|| "Updater endpoint is not configured".to_string())?;
        let manifest = fetch_latest_manifest(&endpoint).await?;
        let manifest_version = manifest.version.trim().to_string();
        if manifest_version.is_empty() {
            return Err("latest.json returned an empty version".to_string());
        }

        if let Some(expected) = expected_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if expected != manifest_version {
                return Err(format!(
                    "Expected update version {} but latest manifest is {}",
                    expected, manifest_version
                ));
            }
        }

        if !is_newer_version(&manifest_version, current_version) {
            return Err(format!(
                "No newer Linux update available (current={}, latest={})",
                current_version, manifest_version
            ));
        }

        let platform_key = current_platform_key(install_kind)?;
        let platform = manifest
            .platforms
            .get(&platform_key)
            .ok_or_else(|| format!("No package found for updater target {}", platform_key))?;

        let (release_notes, release_notes_zh) = split_release_notes(&manifest.notes);
        save_pending_notes(manifest_version.clone(), release_notes, release_notes_zh)?;

        publish(
            &emit_progress,
            &manifest_version,
            "download_started",
            Some(0),
        );
        logger::log_info(&format!(
            "[Updater] Linux managed update download started: version={}, kind={}, target={}",
            manifest_version,
            install_kind.as_str(),
            platform_key
        ));

        let downloaded = download_update_package(
            &emit_progress,
            &manifest_version,
            &platform.url,
            &platform.signature,
            &config.pubkey,
        )
        .await?;

        publish(
            &emit_progress,
            &manifest_version,
            "auth_required",
            Some(100),
        );
        logger::log_info(&format!(
            "[Updater] Linux managed update downloaded, installing: version={}, path={}",
            manifest_version,
            downloaded.display()
        ));

        publish(&emit_progress, &manifest_version, "installing", Some(100));
        install_downloaded_package(install_kind, &downloaded)?;

        publish(&emit_progress, &manifest_version, "completed", Some(100));
        logger::log_info(&format!(
            "[Updater] Linux managed update installed: version={}, kind={}",
            manifest_version,
            install_kind.as_str()
        ));

        Ok(())
    }

    fn publish<EmitProgress>(
        emit_progress: &EmitProgress,
        version: &str,
        phase: &str,
        progress: Option<u8>,
    ) where
        EmitProgress: Fn(LinuxUpdateProgressPayload),
    {
        emit_progress(LinuxUpdateProgressPayload {
            version: version.to_string(),
            phase: phase.to_string(),
            progress,
        });
    }

    async fn fetch_latest_manifest(endpoint: &str) -> Result<LatestManifest, String> {
        let response = reqwest::get(endpoint)
            .await
            .map_err(|error| format!("Failed to fetch latest manifest: {}", error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("Failed to fetch latest manifest: HTTP {}", status));
        }
        response
            .json::<LatestManifest>()
            .await
            .map_err(|error| format!("Failed to parse latest manifest: {}", error))
    }

    async fn download_update_package<EmitProgress>(
        emit_progress: &EmitProgress,
        version: &str,
        url: &str,
        signature: &str,
        pubkey: &str,
    ) -> Result<PathBuf, String>
    where
        EmitProgress: Fn(LinuxUpdateProgressPayload),
    {
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|error| format!("Failed to download update package: {}", error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Failed to download update package: HTTP {}",
                status
            ));
        }

        let content_length = response.content_length().unwrap_or(0);
        let package_path = package_download_path(version, url)?;
        if let Some(parent) = package_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| format!("Failed to create update cache dir: {}", error))?;
        }

        let mut file = fs::File::create(&package_path)
            .await
            .map_err(|error| format!("Failed to create update package file: {}", error))?;
        let mut stream = response.bytes_stream();
        let mut downloaded_bytes = Vec::new();
        let mut downloaded = 0u64;
        let mut last_progress = 0u8;

        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|error| format!("Failed to read update stream: {}", error))?;
            file.write_all(&chunk)
                .await
                .map_err(|error| format!("Failed to write update package: {}", error))?;
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            downloaded_bytes.extend_from_slice(&chunk);

            if content_length > 0 {
                let progress = ((downloaded.saturating_mul(100)) / content_length).min(100) as u8;
                if progress != last_progress {
                    last_progress = progress;
                    publish(emit_progress, version, "downloading", Some(progress));
                }
            }
        }

        file.flush()
            .await
            .map_err(|error| format!("Failed to flush update package: {}", error))?;

        verify_signature(&downloaded_bytes, signature, pubkey)?;
        publish(emit_progress, version, "downloaded", Some(100));
        Ok(package_path)
    }

    fn package_download_path(version: &str, url: &str) -> Result<PathBuf, String> {
        let data_dir = dirs::data_local_dir()
            .map(|dir| dir.join("cockpit-tools").join("updates").join(version))
            .ok_or_else(|| "Failed to resolve local data directory".to_string())?;
        let file_name = Url::parse(url)
            .ok()
            .and_then(|parsed| {
                parsed
                    .path_segments()
                    .and_then(|mut segments| segments.next_back().map(str::to_string))
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("cockpit-tools-update-{}", version));
        Ok(data_dir.join(file_name))
    }

    fn verify_signature(data: &[u8], release_signature: &str, pub_key: &str) -> Result<(), String> {
        let pub_key_decoded = base64_to_string(pub_key)?;
        let public_key = PublicKey::decode(&pub_key_decoded)
            .map_err(|error| format!("Failed to decode updater public key: {}", error))?;
        let signature_decoded = base64_to_string(release_signature)?;
        let signature = Signature::decode(&signature_decoded)
            .map_err(|error| format!("Failed to decode updater signature: {}", error))?;
        public_key
            .verify(data, &signature, true)
            .map_err(|error| format!("Failed to verify updater signature: {}", error))?;
        Ok(())
    }

    fn base64_to_string(value: &str) -> Result<String, String> {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|error| format!("Failed to decode base64 string: {}", error))?;
        std::str::from_utf8(&decoded)
            .map(|text| text.to_string())
            .map_err(|error| format!("Decoded updater value is not UTF-8: {}", error))
    }

    fn split_release_notes(notes: &str) -> (String, String) {
        let normalized = notes.replace("\r\n", "\n").trim().to_string();
        if normalized.is_empty() {
            return (String::new(), String::new());
        }

        let zh_index = normalized.find(ZH_SECTION_HEADER);
        let en_index = normalized.find(EN_SECTION_HEADER);

        match (zh_index, en_index) {
            (Some(zh_pos), Some(en_pos)) if zh_pos < en_pos => (
                normalized[en_pos + EN_SECTION_HEADER.len()..]
                    .trim()
                    .to_string(),
                normalized[zh_pos + ZH_SECTION_HEADER.len()..en_pos]
                    .trim()
                    .to_string(),
            ),
            (Some(zh_pos), Some(en_pos)) => (
                normalized[en_pos + EN_SECTION_HEADER.len()..zh_pos]
                    .trim()
                    .to_string(),
                normalized[zh_pos + ZH_SECTION_HEADER.len()..]
                    .trim()
                    .to_string(),
            ),
            _ => (normalized.clone(), normalized),
        }
    }

    fn current_platform_key(kind: LinuxInstallKind) -> Result<String, String> {
        let arch = match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            other => {
                return Err(format!(
                    "Unsupported Linux architecture for updater: {}",
                    other
                ));
            }
        };

        let suffix = match kind {
            LinuxInstallKind::Deb => "deb",
            LinuxInstallKind::Rpm => "rpm",
            LinuxInstallKind::AppImage => "appimage",
            LinuxInstallKind::Unknown => {
                return Err("Linux install kind is unknown".to_string());
            }
        };

        Ok(format!("linux-{}-{}", arch, suffix))
    }

    fn install_downloaded_package(
        kind: LinuxInstallKind,
        package_path: &Path,
    ) -> Result<(), String> {
        match kind {
            LinuxInstallKind::Deb => install_deb_package(package_path),
            LinuxInstallKind::Rpm => install_rpm_package(package_path),
            LinuxInstallKind::AppImage | LinuxInstallKind::Unknown => Err(format!(
                "Unsupported Linux install kind for managed install: {}",
                kind.as_str()
            )),
        }
    }

    fn install_deb_package(package_path: &Path) -> Result<(), String> {
        let package = package_path.to_string_lossy().to_string();
        let mut attempts = Vec::new();

        if let Err(error) = run_command("pkcon", &["-y", "install-local", &package]) {
            attempts.push(format!("pkcon install-local: {}", error));
        } else {
            return Ok(());
        }

        if let Err(error) = run_command("pkexec", &["apt", "install", "-y", &package]) {
            attempts.push(format!("pkexec apt install: {}", error));
        } else {
            return Ok(());
        }

        if let Err(error) = run_command("pkexec", &["dpkg", "-i", &package]) {
            attempts.push(format!("pkexec dpkg -i: {}", error));
        } else {
            return Ok(());
        }

        Err(format!(
            "Failed to install .deb update package. Attempts: {}",
            attempts.join(" | ")
        ))
    }

    fn install_rpm_package(package_path: &Path) -> Result<(), String> {
        let package = package_path.to_string_lossy().to_string();
        let mut attempts = Vec::new();

        if let Err(error) = run_command("pkcon", &["-y", "install-local", &package]) {
            attempts.push(format!("pkcon install-local: {}", error));
        } else {
            return Ok(());
        }

        if let Err(error) = run_command("pkexec", &["dnf", "install", "-y", &package]) {
            attempts.push(format!("pkexec dnf install: {}", error));
        } else {
            return Ok(());
        }

        if let Err(error) = run_command("pkexec", &["yum", "install", "-y", &package]) {
            attempts.push(format!("pkexec yum install: {}", error));
        } else {
            return Ok(());
        }

        if let Err(error) = run_command("pkexec", &["rpm", "-U", "--replacepkgs", &package]) {
            attempts.push(format!("pkexec rpm -U: {}", error));
        } else {
            return Ok(());
        }

        Err(format!(
            "Failed to install .rpm update package. Attempts: {}",
            attempts.join(" | ")
        ))
    }

    fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
        logger::log_info(&format!(
            "[Updater] Linux install command: {} {}",
            program,
            args.join(" ")
        ));

        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|error| format!("spawn failed: {}", error))?;

        if output.status.success() {
            return Ok(());
        }

        Err(format_command_output(&output))
    }

    fn format_command_output(output: &Output) -> String {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let status = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string());

        match (stdout.is_empty(), stderr.is_empty()) {
            (true, true) => format!("exit status {}", status),
            (false, true) => format!("exit status {}, stdout: {}", status, stdout),
            (true, false) => format!("exit status {}, stderr: {}", status, stderr),
            (false, false) => format!(
                "exit status {}, stdout: {}, stderr: {}",
                status, stdout, stderr
            ),
        }
    }

    fn detect_linux_install_kind() -> LinuxInstallKind {
        if std::env::var_os("APPIMAGE").is_some() {
            return LinuxInstallKind::AppImage;
        }

        let current_exe = match std::env::current_exe() {
            Ok(path) => path,
            Err(_) => return LinuxInstallKind::Unknown,
        };

        let mut candidates = vec![current_exe.clone()];
        if let Ok(canonical) = std::fs::canonicalize(&current_exe) {
            if canonical != current_exe {
                candidates.push(canonical);
            }
        }

        for candidate in candidates {
            if is_managed_by_dpkg(&candidate) {
                return LinuxInstallKind::Deb;
            }
            if is_managed_by_rpm(&candidate) {
                return LinuxInstallKind::Rpm;
            }
            if candidate
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("appimage"))
                .unwrap_or(false)
            {
                return LinuxInstallKind::AppImage;
            }
        }

        LinuxInstallKind::Unknown
    }

    fn is_managed_by_dpkg(path: &Path) -> bool {
        Command::new("dpkg-query")
            .arg("-S")
            .arg(path)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn is_managed_by_rpm(path: &Path) -> bool {
        Command::new("rpm")
            .arg("-qf")
            .arg(path)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn is_newer_version(latest: &str, current: &str) -> bool {
        let parse_version = |value: &str| -> Vec<u32> {
            value
                .split('.')
                .filter_map(|part| part.parse::<u32>().ok())
                .collect()
        };

        let latest_parts = parse_version(latest);
        let current_parts = parse_version(current);

        for index in 0..latest_parts.len().max(current_parts.len()) {
            let latest_part = latest_parts.get(index).copied().unwrap_or(0);
            let current_part = current_parts.get(index).copied().unwrap_or(0);

            if latest_part > current_part {
                return true;
            }
            if latest_part < current_part {
                return false;
            }
        }

        false
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::{LinuxUpdateProgressPayload, UpdateRuntimeInfo, UpdaterPluginConfig};

    pub fn get_update_runtime_info() -> UpdateRuntimeInfo {
        let platform = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "unknown"
        };

        let updater_target = if cfg!(target_os = "windows") {
            let arch = match std::env::consts::ARCH {
                "x86_64" => "x86_64",
                "aarch64" => "aarch64",
                _ => "x86_64",
            };
            Some(format!("windows-{}-nsis", arch))
        } else {
            None
        };

        UpdateRuntimeInfo {
            platform: platform.to_string(),
            linux_install_kind: "unknown".to_string(),
            linux_managed_install_supported: false,
            updater_target,
        }
    }

    pub async fn install_linux_update<SaveNotes, EmitProgress>(
        _config: UpdaterPluginConfig,
        _current_version: &str,
        _expected_version: Option<String>,
        _save_pending_notes: SaveNotes,
        _emit_progress: EmitProgress,
    ) -> Result<(), String>
    where
        SaveNotes: Fn(String, String, String) -> Result<(), String>,
        EmitProgress: Fn(LinuxUpdateProgressPayload),
    {
        Err("Linux managed package update is only supported on Linux".to_string())
    }
}

pub use imp::{get_update_runtime_info, install_linux_update};
