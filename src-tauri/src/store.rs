use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::{normalize_config, validate_app_path, validate_config};
use crate::types::CodexProxyConfig;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Clone)]
pub struct SettingsStore {
    file_path: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    config: CodexProxyConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_app_path: Option<String>,
}

impl SettingsStore {
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            file_path: app_data_dir.join("codex-proxy-settings.json"),
        }
    }

    pub fn get_config(&self) -> CodexProxyConfig {
        self.read()
            .map(|settings| settings.config)
            .unwrap_or_default()
    }

    pub fn save_config(&self, mut config: CodexProxyConfig) -> Result<CodexProxyConfig, String> {
        normalize_config(&mut config)?;
        let selected_app_path = self.read().and_then(|settings| settings.selected_app_path);
        self.write(&StoredSettings {
            config: config.clone(),
            selected_app_path,
        })?;
        Ok(config)
    }

    pub fn selected_app_path(&self) -> Option<String> {
        self.read().and_then(|settings| settings.selected_app_path)
    }

    pub fn save_selected_app_path(&self, value: &Path) -> Result<(), String> {
        let selected = validate_app_path(value)?.to_string_lossy().into_owned();
        self.write(&StoredSettings {
            config: self.get_config(),
            selected_app_path: Some(selected),
        })
    }

    fn read(&self) -> Option<StoredSettings> {
        let text = fs::read_to_string(&self.file_path).ok()?;
        let mut settings: StoredSettings = serde_json::from_str(&text).ok()?;
        validate_config(&settings.config).ok()?;
        normalize_config(&mut settings.config).ok()?;
        if let Some(path) = settings.selected_app_path.as_deref() {
            validate_app_path(Path::new(path)).ok()?;
        }
        Some(settings)
    }

    fn write(&self, settings: &StoredSettings) -> Result<(), String> {
        let parent = self
            .file_path
            .parent()
            .ok_or_else(|| "配置目录无效。".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
        let temporary = self.file_path.with_extension("json.tmp");
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?
        );
        fs::write(&temporary, text).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
        fs::rename(temporary, &self.file_path).map_err(|error| error.to_string())
    }
}
