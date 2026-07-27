use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

#[derive(Clone)]
pub struct AppLogger {
    directory: PathBuf,
    file_path: PathBuf,
}

impl AppLogger {
    pub fn new(home: &Path) -> Self {
        let directory = home.join("Library/Logs/CodexProxy");
        let file_path = directory.join("launcher.log");
        Self {
            directory,
            file_path,
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn ensure_directory(&self) -> Result<(), String> {
        fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
        fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())
    }

    pub fn log(&self, level: &str, message: &str, detail: Option<&str>) {
        if self.ensure_directory().is_err() {
            return;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_millis())
            .unwrap_or_default();
        let suffix = detail.map(|value| format!(" {value}")).unwrap_or_default();
        let line = redact_sensitive(&format!("{timestamp} [{level}] {message}{suffix}\n"));
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&self.file_path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

fn redact_sensitive(input: &str) -> String {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        vec![
            (
                Regex::new(r"(?i)\b(authorization)\b\s*[:=]\s*(?:Bearer\s+)?[^\s,;]+").unwrap(),
                "$1=[REDACTED]",
            ),
            (
                Regex::new(
                    r"(?i)\b(cookie|set-cookie|openai[_-]?token|api[_-]?key)\b\s*[:=]\s*[^\s,;]+",
                )
                .unwrap(),
                "$1=[REDACTED]",
            ),
            (
                Regex::new(r"(?i)\b(Bearer)\s+[A-Za-z0-9._~+/=-]+").unwrap(),
                "$1 [REDACTED]",
            ),
            (
                Regex::new(r"(?i)\b(https?|socks5)://([^/\s:@]+):([^@\s/]+)@").unwrap(),
                "$1://[REDACTED]@",
            ),
            (
                Regex::new(r"\bsk-[A-Za-z0-9_-]{12,}\b").unwrap(),
                "[REDACTED_TOKEN]",
            ),
        ]
    });
    patterns
        .iter()
        .fold(input.to_owned(), |value, (regex, replacement)| {
            regex.replace_all(&value, *replacement).into_owned()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials() {
        let result = redact_sensitive(
            "Authorization: Bearer secret Cookie=session socks5://u:p@localhost sk-abcdefghijklmnop",
        );
        assert!(!result.contains("secret"));
        assert!(!result.contains("session"));
        assert!(!result.contains("abcdefghijklmnop"));
        assert!(result.contains("[REDACTED]"));
    }
}
