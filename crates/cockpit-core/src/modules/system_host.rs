use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn downloads_dir() -> Result<PathBuf, String> {
    if let Some(dir) = dirs::download_dir() {
        return Ok(dir);
    }
    if let Some(home) = dirs::home_dir() {
        return Ok(home.join("Downloads"));
    }
    Err("failed to get downloads directory".to_string())
}

pub fn home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "failed to get home directory".to_string())
}

pub fn save_text_file(path: impl AsRef<Path>, content: &str) -> Result<(), String> {
    fs::write(path.as_ref(), content).map_err(|err| format!("write text file failed: {err}"))
}

pub fn read_text_file(path: impl AsRef<Path>) -> Result<String, String> {
    fs::read_to_string(path.as_ref()).map_err(|err| format!("read text file failed: {err}"))
}

pub fn ensure_folder(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    if !path.exists() {
        fs::create_dir_all(path).map_err(|err| format!("create folder failed: {err}"))?;
    }
    if !path.is_dir() {
        return Err(format!("path is not a directory: {}", path.display()));
    }
    Ok(())
}

pub fn open_path_in_system(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|err| format!("open path failed: {err}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|err| format!("open path failed: {err}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|err| format!("open path failed: {err}"))?;
    }
    Ok(())
}

pub fn open_existing_path(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(format!(
            "path does not exist on service host: {}",
            path.display()
        ));
    }
    open_path_in_system(path)
}

pub fn open_folder(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    ensure_folder(path)?;
    open_path_in_system(path)
}

pub fn open_data_folder() -> Result<(), String> {
    let path = crate::modules::account::get_data_dir()?;
    open_path_in_system(path)
}

pub fn detect_app_path(app: &str, force: bool) -> Result<Option<String>, String> {
    match app {
        "windsurf" => {
            Ok(crate::modules::windsurf_instance::detect_and_save_windsurf_launch_path(force))
        }
        "kiro" => Ok(crate::modules::kiro_instance::detect_and_save_kiro_launch_path(force)),
        "cursor" => Ok(crate::modules::cursor_instance::detect_and_save_cursor_launch_path(force)),
        "antigravity" | "codex" | "zed" | "vscode" | "codebuddy" | "codebuddy_cn" | "qoder"
        | "trae" | "opencode" | "workbuddy" => Ok(
            crate::modules::process::detect_and_save_app_path(app, force),
        ),
        _ => Err("未知应用类型".to_string()),
    }
}

pub fn available_terminals() -> Vec<String> {
    let mut available = vec!["system".to_string()];

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        let terminals = [
            (
                "Terminal",
                vec![
                    "/System/Applications/Utilities/Terminal.app".to_string(),
                    "/Applications/Utilities/Terminal.app".to_string(),
                ],
            ),
            (
                "iTerm2",
                vec![
                    "/Applications/iTerm.app".to_string(),
                    "/Applications/iTerm 2.app".to_string(),
                    format!("{}/Applications/iTerm.app", home),
                ],
            ),
            (
                "Warp",
                vec![
                    "/Applications/Warp.app".to_string(),
                    format!("{}/Applications/Warp.app", home),
                ],
            ),
            (
                "Ghostty",
                vec![
                    "/Applications/Ghostty.app".to_string(),
                    format!("{}/Applications/Ghostty.app", home),
                ],
            ),
            (
                "WezTerm",
                vec![
                    "/Applications/WezTerm.app".to_string(),
                    format!("{}/Applications/WezTerm.app", home),
                ],
            ),
            (
                "Kitty",
                vec![
                    "/Applications/Kitty.app".to_string(),
                    format!("{}/Applications/Kitty.app", home),
                ],
            ),
            (
                "Alacritty",
                vec![
                    "/Applications/Alacritty.app".to_string(),
                    format!("{}/Applications/Alacritty.app", home),
                ],
            ),
        ];
        for (name, paths) in terminals {
            if paths
                .iter()
                .any(|path| !path.is_empty() && Path::new(path).exists())
            {
                available.push(name.to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        for name in ["cmd", "powershell", "pwsh", "wt"] {
            if is_command_available(name) {
                available.push(name.to_string());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for name in [
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal",
            "xterm",
            "alacritty",
            "kitty",
        ] {
            if is_command_available(name) {
                available.push(name.to_string());
            }
        }
    }

    available
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn is_command_available(cmd: &str) -> bool {
    #[cfg(target_os = "windows")]
    let check_cmd = "where";
    #[cfg(target_os = "linux")]
    let check_cmd = "which";

    let mut command = std::process::Command::new(check_cmd);
    command
        .arg(cmd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.status().map(|s| s.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        available_terminals, detect_app_path, ensure_folder, read_text_file, save_text_file,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "cockpit-core-system-host-{name}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create test dir");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn saves_and_reads_text_file() {
        let dir = TestDir::new("text");
        let path = dir.path.join("notes.txt");

        save_text_file(&path, "hello").expect("save text");

        assert_eq!(read_text_file(&path).expect("read text"), "hello");
    }

    #[test]
    fn ensure_folder_creates_missing_directory() {
        let dir = TestDir::new("folder");
        let path = dir.path.join("nested").join("target");

        ensure_folder(&path).expect("ensure folder");

        assert!(path.is_dir());
    }

    #[test]
    fn ensure_folder_rejects_file_path() {
        let dir = TestDir::new("file-path");
        let path = dir.path.join("target");
        fs::write(&path, "content").expect("write file");

        let err = ensure_folder(&path).expect_err("file path error");

        assert!(err.starts_with("path is not a directory:"));
    }

    #[test]
    fn available_terminals_always_contains_system() {
        let terminals = available_terminals();
        assert_eq!(terminals.first().map(String::as_str), Some("system"));
    }

    #[test]
    fn detect_app_path_rejects_unknown_app() {
        let err = detect_app_path("unknown", false).expect_err("unknown app should be rejected");

        assert_eq!(err, "未知应用类型");
    }
}
