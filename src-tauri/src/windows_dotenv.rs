use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Backup {
    original: Option<Vec<u8>>,
    injected: Vec<u8>,
}

pub struct DotenvOverlay {
    path: PathBuf,
    backup_path: PathBuf,
    restored: bool,
}

impl DotenvOverlay {
    pub fn stage(home: &Path, environment: &[(String, String)]) -> Result<Self, String> {
        let directory = home.join(".codex");
        fs::create_dir_all(&directory)
            .map_err(|error| format!("无法访问 Codex 配置目录：{error}"))?;
        let path = directory.join(".env");
        let backup_path = directory.join(".env.codex-proxy-launcher-backup.json");
        recover(&path, &backup_path)?;

        let original = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("无法读取 Codex .env：{error}")),
        };
        if original
            .as_deref()
            .is_some_and(|bytes| std::str::from_utf8(bytes).is_err())
        {
            return Err("现有 Codex .env 不是 UTF-8，无法安全追加临时代理设置。".into());
        }
        let mut injected = original.clone().unwrap_or_default();
        if !injected.is_empty() && !injected.ends_with(b"\n") {
            injected.push(b'\n');
        }
        injected.extend_from_slice(b"# Temporary Codex proxy launcher settings\n");
        for (name, value) in environment {
            if !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || value.contains(['\r', '\n', '\0'])
            {
                return Err("代理环境变量格式无效。".into());
            }
            injected.extend_from_slice(format!("{name}={value}\n").as_bytes());
        }

        let backup = Backup { original, injected };
        let encoded = serde_json::to_vec(&backup).map_err(|error| error.to_string())?;
        fs::write(&backup_path, encoded)
            .map_err(|error| format!("无法备份 Codex .env：{error}"))?;
        if let Err(error) = fs::write(&path, &backup.injected) {
            let _ = recover(&path, &backup_path);
            return Err(format!("无法临时写入 Codex .env：{error}"));
        }
        Ok(Self {
            path,
            backup_path,
            restored: false,
        })
    }

    pub fn restore(&mut self) -> Result<(), String> {
        recover(&self.path, &self.backup_path)?;
        self.restored = true;
        Ok(())
    }
}

pub fn recover_interrupted_launch(home: &Path) -> Result<(), String> {
    let directory = home.join(".codex");
    recover(
        &directory.join(".env"),
        &directory.join(".env.codex-proxy-launcher-backup.json"),
    )
}

impl Drop for DotenvOverlay {
    fn drop(&mut self) {
        if !self.restored {
            let _ = self.restore();
        }
    }
}

fn recover(path: &Path, backup_path: &Path) -> Result<(), String> {
    let encoded = match fs::read(backup_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("无法读取 Codex .env 备份：{error}")),
    };
    let backup: Backup = serde_json::from_slice(&encoded)
        .map_err(|_| format!("Codex .env 备份损坏，请检查 {}", backup_path.display()))?;
    let current = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("无法读取 Codex .env：{error}")),
    };
    if current.as_deref() != Some(backup.injected.as_slice()) && current != backup.original {
        return Err(format!(
            "Codex .env 在代理启动期间发生修改，已保留原文件和备份，请检查 {}",
            backup_path.display()
        ));
    }
    match backup.original {
        Some(bytes) => {
            fs::write(path, bytes).map_err(|error| format!("无法恢复 Codex .env：{error}"))?
        }
        None => {
            if path.exists() {
                fs::remove_file(path)
                    .map_err(|error| format!("无法清理临时 Codex .env：{error}"))?;
            }
        }
    }
    fs::remove_file(backup_path).map_err(|error| format!("无法清理 Codex .env 备份：{error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_and_restores_existing_dotenv() {
        let root = std::env::temp_dir().join(format!("codex-dotenv-test-{}", std::process::id()));
        let directory = root.join(".codex");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(".env");
        fs::write(&path, b"MY_SETTING=keep").unwrap();
        let mut overlay = DotenvOverlay::stage(
            &root,
            &[("HTTPS_PROXY".into(), "http://127.0.0.1:7897".into())],
        )
        .unwrap();
        let staged = fs::read_to_string(&path).unwrap();
        assert!(staged.contains("MY_SETTING=keep\n"));
        assert!(staged.contains("HTTPS_PROXY=http://127.0.0.1:7897\n"));
        overlay.restore().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"MY_SETTING=keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restores_missing_dotenv_after_interrupted_launch() {
        let root =
            std::env::temp_dir().join(format!("codex-dotenv-recover-{}", std::process::id()));
        let overlay = DotenvOverlay::stage(
            &root,
            &[("HTTP_PROXY".into(), "http://127.0.0.1:7897".into())],
        )
        .unwrap();
        std::mem::forget(overlay);
        let mut next = DotenvOverlay::stage(
            &root,
            &[("HTTP_PROXY".into(), "http://127.0.0.1:7890".into())],
        )
        .unwrap();
        next.restore().unwrap();
        assert!(!root.join(".codex/.env").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
