use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::modules::{config, logger, process};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const ANTIGRAVITY_VERSION_BADGE_TIMEOUT_MS: u64 = 1200;
pub const ANTIGRAVITY_VERSION_FULL_SCAN_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntigravityInstalledVersionInfo {
    pub product_name: String,
    pub version: String,
    pub app_path: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityVersionScanMode {
    Quick,
    Full,
}

static ANTIGRAVITY_VERSION_INFO_CACHE: OnceLock<
    Mutex<HashMap<String, AntigravityInstalledVersionInfo>>,
> = OnceLock::new();

fn trim_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn json_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .and_then(trim_non_empty)
    })
}

#[cfg(target_os = "macos")]
fn normalize_macos_app_root_for_metadata(path: &Path) -> Option<PathBuf> {
    let path_str = path.to_string_lossy();
    let app_idx = path_str.find(".app")?;
    let root = PathBuf::from(&path_str[..app_idx + 4]);
    root.exists().then_some(root)
}

#[cfg(target_os = "macos")]
fn read_macos_plist_string(path: &Path, key: &str) -> Option<String> {
    let output = std::process::Command::new("plutil")
        .arg("-p")
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let prefix = format!("\"{}\"", key);
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with(&prefix) {
            continue;
        }
        let value = line.split("=>").nth(1)?.trim().trim_matches('"');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn antigravity_product_json_candidates(root: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            root.join("Contents")
                .join("Resources")
                .join("app")
                .join("product.json"),
            root.join("resources").join("app").join("product.json"),
            root.join("app").join("product.json"),
        ]
    }

    #[cfg(not(target_os = "macos"))]
    {
        vec![
            root.join("resources").join("app").join("product.json"),
            root.join("app").join("product.json"),
        ]
    }
}

fn read_antigravity_product_json_metadata(root: &Path) -> Option<AntigravityInstalledVersionInfo> {
    for path in antigravity_product_json_candidates(root) {
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let Some(version) = json_string_field(&value, &["ideVersion", "version"]) else {
            continue;
        };
        let product_name = json_string_field(
            &value,
            &["nameShort", "nameLong", "productName", "applicationName"],
        )
        .unwrap_or_else(|| "Antigravity".to_string());
        return Some(AntigravityInstalledVersionInfo {
            product_name,
            version,
            app_path: root.to_string_lossy().to_string(),
            source: "product.json".to_string(),
        });
    }
    None
}

#[cfg(target_os = "macos")]
fn read_antigravity_macos_bundle_metadata(root: &Path) -> Option<AntigravityInstalledVersionInfo> {
    let plist_path = root.join("Contents").join("Info.plist");
    if !plist_path.exists() {
        return None;
    }

    let version = read_macos_plist_string(&plist_path, "CFBundleShortVersionString")
        .or_else(|| read_macos_plist_string(&plist_path, "CFBundleVersion"))?;
    let product_name = read_macos_plist_string(&plist_path, "CFBundleDisplayName")
        .or_else(|| read_macos_plist_string(&plist_path, "CFBundleName"))
        .unwrap_or_else(|| "Antigravity".to_string());

    Some(AntigravityInstalledVersionInfo {
        product_name,
        version,
        app_path: root.to_string_lossy().to_string(),
        source: "Info.plist".to_string(),
    })
}

#[cfg(target_os = "windows")]
fn find_antigravity_windows_exe(root: &Path) -> Option<PathBuf> {
    if root.is_file() {
        return Some(root.to_path_buf());
    }

    let candidates = [
        root.join("Antigravity.exe"),
        root.join("Antigravity IDE.exe"),
        root.join("antigravity.exe"),
        root.join("antigravity-ide.exe"),
        root.join("Electron.exe"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

#[cfg(target_os = "windows")]
fn read_powershell_json_for_antigravity_exe(
    exe_path: &Path,
    script: &str,
) -> Option<serde_json::Value> {
    let mut command = std::process::Command::new("powershell");
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .env("COCKPIT_ANTIGRAVITY_EXE_PATH", exe_path.as_os_str())
        .output()
        .ok()?;
    if !output.status.success() {
        logger::log_warn(&format!(
            "[Antigravity] Windows version metadata PowerShell probe failed: status={}",
            output.status
        ));
        return None;
    }

    serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()
}

#[cfg(target_os = "windows")]
fn build_antigravity_windows_version_info(
    value: serde_json::Value,
    exe_path: &Path,
    source: &str,
) -> Option<AntigravityInstalledVersionInfo> {
    let version = json_string_field(&value, &["ProductVersion", "FileVersion", "DisplayVersion"])?;
    let product_name = json_string_field(&value, &["ProductName", "DisplayName"])
        .unwrap_or_else(|| "Antigravity".to_string());

    Some(AntigravityInstalledVersionInfo {
        product_name,
        version,
        app_path: exe_path.to_string_lossy().to_string(),
        source: source.to_string(),
    })
}

#[cfg(target_os = "windows")]
fn read_antigravity_windows_uninstall_metadata(
    exe_path: &Path,
) -> Option<AntigravityInstalledVersionInfo> {
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

function Normalize-RegistryPath([string]$value) {
  if ([string]::IsNullOrWhiteSpace($value)) { return $null }
  $clean = $value.Trim().Trim('"')
  $clean = $clean -replace ',\d+$',''
  try { return [System.IO.Path]::GetFullPath($clean) } catch { return $clean }
}

$exe = [Environment]::GetEnvironmentVariable('COCKPIT_ANTIGRAVITY_EXE_PATH', 'Process')
if ([string]::IsNullOrWhiteSpace($exe)) { exit 3 }
$exe = [System.IO.Path]::GetFullPath($exe)

$roots = @(
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)

$match = Get-ItemProperty -Path $roots -ErrorAction SilentlyContinue |
  Where-Object {
    $_.DisplayName -like 'Antigravity*' -and (
      ((Normalize-RegistryPath $_.DisplayIcon) -ieq $exe) -or
      ($_.InstallLocation -and $exe.StartsWith(
        (Normalize-RegistryPath $_.InstallLocation).TrimEnd('\') + '\',
        [System.StringComparison]::OrdinalIgnoreCase
      ))
    )
  } |
  Select-Object -First 1

if (-not $match) { exit 4 }

[pscustomobject]@{
  DisplayName = $match.DisplayName
  DisplayVersion = $match.DisplayVersion
} | ConvertTo-Json -Compress
"#;

    let value = read_powershell_json_for_antigravity_exe(exe_path, script)?;
    build_antigravity_windows_version_info(value, exe_path, "UninstallRegistry")
}

#[cfg(target_os = "windows")]
fn read_antigravity_windows_exe_metadata(root: &Path) -> Option<AntigravityInstalledVersionInfo> {
    let exe_path = find_antigravity_windows_exe(root)?;
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
$p = [Environment]::GetEnvironmentVariable('COCKPIT_ANTIGRAVITY_EXE_PATH', 'Process')
if ([string]::IsNullOrWhiteSpace($p)) { exit 3 }
if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { exit 2 }
$v = (Get-Item -LiteralPath $p).VersionInfo
if ([string]::IsNullOrWhiteSpace($v.ProductVersion) -and [string]::IsNullOrWhiteSpace($v.FileVersion)) { exit 4 }
[pscustomobject]@{
  ProductName = $v.ProductName
  ProductVersion = $v.ProductVersion
  FileVersion = $v.FileVersion
} | ConvertTo-Json -Compress
"#;

    read_powershell_json_for_antigravity_exe(&exe_path, script)
        .and_then(|value| build_antigravity_windows_version_info(value, &exe_path, "VersionInfo"))
        .or_else(|| read_antigravity_windows_uninstall_metadata(&exe_path))
}

fn normalize_antigravity_metadata_root(path: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Some(root) = normalize_macos_app_root_for_metadata(path) {
            return Some(root);
        }
    }

    if path.is_file() {
        return path.parent().map(Path::to_path_buf);
    }
    if path.is_dir() {
        return Some(path.to_path_buf());
    }
    None
}

fn push_unique_antigravity_candidate(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    let normalized_key = path.to_string_lossy().to_ascii_lowercase();
    let exists = candidates
        .iter()
        .any(|item| item.to_string_lossy().to_ascii_lowercase() == normalized_key);
    if !exists {
        candidates.push(path);
    }
}

fn normalize_antigravity_metadata_target(target: Option<&str>) -> Option<&'static str> {
    match target.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "antigravity" => Some("antigravity"),
        "antigravity_ide" | "antigravity-ide" | "ide" => Some("antigravity_ide"),
        _ => None,
    }
}

pub fn normalize_antigravity_version_scan_mode(raw: Option<&str>) -> AntigravityVersionScanMode {
    match raw.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "full" | "complete" => AntigravityVersionScanMode::Full,
        _ => AntigravityVersionScanMode::Quick,
    }
}

pub fn antigravity_version_timeout_ms(scan_mode: AntigravityVersionScanMode) -> u64 {
    match scan_mode {
        AntigravityVersionScanMode::Quick => ANTIGRAVITY_VERSION_BADGE_TIMEOUT_MS,
        AntigravityVersionScanMode::Full => ANTIGRAVITY_VERSION_FULL_SCAN_TIMEOUT_MS,
    }
}

fn antigravity_version_cache() -> &'static Mutex<HashMap<String, AntigravityInstalledVersionInfo>> {
    ANTIGRAVITY_VERSION_INFO_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn antigravity_version_cache_key(target: Option<&str>) -> String {
    normalize_antigravity_metadata_target(target)
        .unwrap_or("all")
        .to_string()
}

fn cache_antigravity_installed_version_info(
    target: Option<&str>,
    info: &AntigravityInstalledVersionInfo,
) {
    if let Ok(mut cache) = antigravity_version_cache().lock() {
        cache.insert(antigravity_version_cache_key(target), info.clone());
    }
}

pub fn get_cached_antigravity_installed_version_info_for_target(
    target: Option<&str>,
) -> Option<AntigravityInstalledVersionInfo> {
    antigravity_version_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&antigravity_version_cache_key(target)).cloned())
}

fn antigravity_metadata_root_matches_target(root: &Path, target: Option<&str>) -> bool {
    let Some(target) = normalize_antigravity_metadata_target(target) else {
        return true;
    };
    let value = root.to_string_lossy().to_ascii_lowercase();
    match target {
        "antigravity" => {
            value.contains("antigravity.app")
                || value.ends_with("antigravity")
                || value.ends_with("antigravity.exe")
                || (root.is_dir()
                    && (root.join("Antigravity.exe").exists()
                        || root.join("antigravity.exe").exists()))
        }
        "antigravity_ide" => {
            value.contains("antigravity ide.app")
                || value.contains("antigravity ide")
                || value.contains("antigravity-ide")
                || (root.is_dir()
                    && (root.join("Antigravity IDE.exe").exists()
                        || root.join("antigravity-ide.exe").exists()))
        }
        _ => true,
    }
}

fn antigravity_metadata_candidates(
    target: Option<&str>,
    scan_mode: AntigravityVersionScanMode,
) -> Vec<PathBuf> {
    #[cfg(not(target_os = "windows"))]
    let _ = scan_mode;

    let mut candidates = Vec::new();
    let config_path = config::get_user_config().antigravity_app_path;
    let config_path = config_path.trim();
    if !config_path.is_empty() {
        let config_path = Path::new(config_path);
        if let Some(root) = normalize_antigravity_metadata_root(config_path) {
            if antigravity_metadata_root_matches_target(&root, target) {
                push_unique_antigravity_candidate(&mut candidates, root);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let paths: &[&str] = match normalize_antigravity_metadata_target(target) {
            Some("antigravity") => &["/Applications/Antigravity.app"],
            Some("antigravity_ide") => &["/Applications/Antigravity IDE.app"],
            _ => &[
                "/Applications/Antigravity.app",
                "/Applications/Antigravity IDE.app",
            ],
        };
        for path in paths {
            let path = PathBuf::from(path);
            if path.exists() {
                push_unique_antigravity_candidate(&mut candidates, path);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let paths: &[&str] = match normalize_antigravity_metadata_target(target) {
            Some("antigravity") => &["/usr/share/antigravity", "/opt/antigravity"],
            Some("antigravity_ide") => &["/usr/share/antigravity-ide", "/opt/antigravity-ide"],
            _ => &[
                "/usr/share/antigravity",
                "/usr/share/antigravity-ide",
                "/opt/antigravity",
                "/opt/antigravity-ide",
            ],
        };
        for path in paths {
            let path = PathBuf::from(path);
            if path.exists() {
                push_unique_antigravity_candidate(&mut candidates, path);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            let base = PathBuf::from(local_appdata).join("Programs");
            match normalize_antigravity_metadata_target(target) {
                Some("antigravity") => roots.push(base.join("Antigravity")),
                Some("antigravity_ide") => roots.push(base.join("Antigravity IDE")),
                _ => {
                    roots.push(base.join("Antigravity"));
                    roots.push(base.join("Antigravity IDE"));
                }
            }
        }
        if let Ok(program_files) = std::env::var("PROGRAMFILES") {
            let base = PathBuf::from(program_files);
            match normalize_antigravity_metadata_target(target) {
                Some("antigravity") => roots.push(base.join("Antigravity")),
                Some("antigravity_ide") => roots.push(base.join("Antigravity IDE")),
                _ => {
                    roots.push(base.join("Antigravity"));
                    roots.push(base.join("Antigravity IDE"));
                }
            }
        }
        if let Ok(program_files_x86) = std::env::var("PROGRAMFILES(X86)") {
            let base = PathBuf::from(program_files_x86);
            match normalize_antigravity_metadata_target(target) {
                Some("antigravity") => roots.push(base.join("Antigravity")),
                Some("antigravity_ide") => roots.push(base.join("Antigravity IDE")),
                _ => {
                    roots.push(base.join("Antigravity"));
                    roots.push(base.join("Antigravity IDE"));
                }
            }
        }
        for path in roots {
            if path.exists() {
                push_unique_antigravity_candidate(&mut candidates, path);
            }
        }

        if scan_mode == AntigravityVersionScanMode::Full {
            if let Some(path) = process::detect_antigravity_exec_path() {
                if let Some(root) = normalize_antigravity_metadata_root(&path) {
                    if antigravity_metadata_root_matches_target(&root, target) {
                        push_unique_antigravity_candidate(&mut candidates, root);
                    }
                }
            }
        }
    }

    candidates
}

fn resolve_antigravity_installed_version_info_for_target_with_mode(
    target: Option<&str>,
    scan_mode: AntigravityVersionScanMode,
) -> Option<AntigravityInstalledVersionInfo> {
    for root in antigravity_metadata_candidates(target, scan_mode) {
        if let Some(info) = read_antigravity_product_json_metadata(&root) {
            return Some(info);
        }

        #[cfg(target_os = "macos")]
        if let Some(info) = read_antigravity_macos_bundle_metadata(&root) {
            return Some(info);
        }

        #[cfg(target_os = "windows")]
        if scan_mode == AntigravityVersionScanMode::Full {
            if let Some(info) = read_antigravity_windows_exe_metadata(&root) {
                return Some(info);
            }
        }
    }

    None
}

pub fn resolve_antigravity_installed_version_info_for_target_with_scan_mode(
    target: Option<&str>,
    scan_mode: AntigravityVersionScanMode,
) -> Option<AntigravityInstalledVersionInfo> {
    let info = resolve_antigravity_installed_version_info_for_target_with_mode(target, scan_mode);
    if let Some(ref value) = info {
        cache_antigravity_installed_version_info(target, value);
    }
    info
}

pub fn resolve_antigravity_installed_version_info_for_target(
    target: Option<&str>,
) -> Option<AntigravityInstalledVersionInfo> {
    resolve_antigravity_installed_version_info_for_target_with_scan_mode(
        target,
        AntigravityVersionScanMode::Full,
    )
}

pub fn resolve_antigravity_installed_version_info_quick_for_target(
    target: Option<&str>,
) -> Option<AntigravityInstalledVersionInfo> {
    resolve_antigravity_installed_version_info_for_target_with_scan_mode(
        target,
        AntigravityVersionScanMode::Quick,
    )
}
