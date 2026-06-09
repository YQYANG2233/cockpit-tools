use std::fs;
use std::path::{Path, PathBuf};

use crate::modules::logger;

pub fn backup_corrupted_file(path: impl AsRef<Path>) -> Result<Option<PathBuf>, String> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(None);
    }
    if path.is_dir() {
        return Err("refusing to backup a directory as a corrupted file".to_string());
    }

    let timestamp = chrono::Utc::now().timestamp();
    let backup_path = PathBuf::from(format!(
        "{}.corrupted.{}",
        path.to_string_lossy(),
        timestamp
    ));
    fs::rename(path, &backup_path).map_err(|err| format!("backup corrupted file failed: {err}"))?;

    logger::log_info(&format!(
        "Backed up corrupted file: {} -> {}",
        path.display(),
        backup_path.display()
    ));
    Ok(Some(backup_path))
}

#[cfg(test)]
mod tests {
    use super::backup_corrupted_file;
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
                "cockpit-core-corrupted-file-{name}-{}-{nanos}",
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
    fn missing_file_is_noop() {
        let dir = TestDir::new("missing");
        let missing = dir.path.join("missing.json");
        let backup = backup_corrupted_file(&missing).expect("backup result");
        assert!(backup.is_none());
        assert!(!missing.exists());
    }

    #[test]
    fn existing_file_is_renamed_to_corrupted_backup() {
        let dir = TestDir::new("backup");
        let file = dir.path.join("accounts.json");
        fs::write(&file, b"broken json").expect("write test file");

        let backup = backup_corrupted_file(&file)
            .expect("backup result")
            .expect("backup path");

        assert!(!file.exists());
        assert!(backup.exists());
        assert_eq!(fs::read(&backup).expect("read backup"), b"broken json");
        assert!(backup
            .to_string_lossy()
            .starts_with(&format!("{}.corrupted.", file.to_string_lossy())));
    }

    #[test]
    fn directory_is_rejected() {
        let dir = TestDir::new("directory");
        let child_dir = dir.path.join("accounts.json");
        fs::create_dir_all(&child_dir).expect("create child dir");

        let err = backup_corrupted_file(&child_dir).expect_err("directory error");

        assert_eq!(err, "refusing to backup a directory as a corrupted file");
        assert!(child_dir.exists());
    }
}
