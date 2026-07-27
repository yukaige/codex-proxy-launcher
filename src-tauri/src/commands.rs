use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

use arboard::Clipboard;
use tauri::{Manager, State};

use crate::detector;
use crate::launcher::{self, LauncherState};
use crate::logger::AppLogger;
use crate::proxy;
use crate::store::SettingsStore;
use crate::types::{
    ActionResult, CodexAppInfo, CodexProxyConfig, CodexStatus, LaunchResult, ProxyTestResult,
    TrafficVerificationResult,
};

#[derive(Clone)]
pub struct AppState {
    store: SettingsStore,
    logger: AppLogger,
    home: PathBuf,
    launcher: Arc<Mutex<LauncherState>>,
}

fn detect_current(state: &AppState) -> CodexAppInfo {
    let preferred = state.store.selected_app_path();
    detector::detect(preferred.as_deref(), &state.home, &state.logger)
}

async fn blocking<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<CodexProxyConfig, String> {
    let state = state.inner().clone();
    blocking(move || Ok(state.store.get_config())).await
}

#[tauri::command]
pub async fn save_config(
    config: CodexProxyConfig,
    state: State<'_, AppState>,
) -> Result<CodexProxyConfig, String> {
    let state = state.inner().clone();
    blocking(move || state.store.save_config(config)).await
}

#[tauri::command]
pub async fn detect_codex(state: State<'_, AppState>) -> Result<CodexAppInfo, String> {
    let state = state.inner().clone();
    blocking(move || Ok(detect_current(&state))).await
}

#[tauri::command]
pub async fn choose_codex(state: State<'_, AppState>) -> Result<CodexAppInfo, String> {
    let state = state.inner().clone();
    blocking(move || {
        let selected = rfd::FileDialog::new()
            .set_title("选择 Codex.app")
            .set_directory("/Applications")
            .add_filter("macOS 应用", &["app"])
            .pick_file();
        let Some(path) = selected else {
            return Ok(detect_current(&state));
        };
        let inspected = detector::inspect(&path, &state.logger);
        if inspected.executable_path.is_some() {
            state.store.save_selected_app_path(&path)?;
        }
        Ok(inspected)
    })
    .await
}

#[tauri::command]
pub async fn test_proxy(
    config: CodexProxyConfig,
    state: State<'_, AppState>,
) -> Result<ProxyTestResult, String> {
    let state = state.inner().clone();
    blocking(move || Ok(proxy::test(&config, &state.logger))).await
}

#[tauri::command]
pub async fn copy_launch_script(
    config: CodexProxyConfig,
    state: State<'_, AppState>,
) -> Result<ActionResult, String> {
    let state = state.inner().clone();
    blocking(move || {
        let config = state.store.save_config(config)?;
        let info = detect_current(&state);
        state.logger.ensure_directory()?;
        let script = launcher::build_launch_script(&info, &config, state.logger.directory())?;
        Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(script))
            .map_err(|error| format!("无法写入剪贴板：{error}"))?;
        Ok(ActionResult {
            success: true,
            message: "启动脚本已复制，可直接粘贴到终端运行。".into(),
        })
    })
    .await
}

#[tauri::command]
pub async fn launch_codex(
    config: CodexProxyConfig,
    state: State<'_, AppState>,
) -> Result<LaunchResult, String> {
    let state = state.inner().clone();
    blocking(move || {
        let config = state.store.save_config(config)?;
        let info = detect_current(&state);
        Ok(launcher::launch_with_proxy(
            &info,
            &config,
            &state.launcher,
            &state.logger,
        ))
    })
    .await
}

#[tauri::command]
pub async fn launch_codex_directly(state: State<'_, AppState>) -> Result<LaunchResult, String> {
    let state = state.inner().clone();
    blocking(move || {
        let info = detect_current(&state);
        Ok(launcher::launch_directly(
            &info,
            &state.launcher,
            &state.logger,
        ))
    })
    .await
}

#[tauri::command]
pub async fn verify_proxy_traffic(
    config: CodexProxyConfig,
    state: State<'_, AppState>,
) -> Result<TrafficVerificationResult, String> {
    let state = state.inner().clone();
    blocking(move || {
        let config = state.store.save_config(config)?;
        Ok(launcher::verify_proxy_traffic(
            &config,
            &state.launcher,
            &state.logger,
        ))
    })
    .await
}

#[tauri::command]
pub async fn stop_codex(state: State<'_, AppState>) -> Result<ActionResult, String> {
    let state = state.inner().clone();
    blocking(move || {
        let info = detect_current(&state);
        Ok(launcher::stop(&info, &state.logger))
    })
    .await
}

#[tauri::command]
pub async fn get_codex_status(state: State<'_, AppState>) -> Result<CodexStatus, String> {
    let state = state.inner().clone();
    blocking(move || {
        let info = detect_current(&state);
        Ok(launcher::status(&info, &state.launcher))
    })
    .await
}

#[tauri::command]
pub async fn open_logs(state: State<'_, AppState>) -> Result<ActionResult, String> {
    let state = state.inner().clone();
    blocking(move || {
        state.logger.ensure_directory()?;
        let status = Command::new("/usr/bin/open")
            .arg(state.logger.directory())
            .status()
            .map_err(|error| error.to_string())?;
        Ok(ActionResult {
            success: status.success(),
            message: if status.success() {
                "已打开日志目录。".into()
            } else {
                format!("无法打开日志目录：{status}")
            },
        })
    })
    .await
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or("无法确定用户主目录。")?;
            app.manage(AppState {
                store: SettingsStore::new(&app_data),
                logger: AppLogger::new(&home),
                home,
                launcher: Arc::new(Mutex::new(LauncherState::default())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            detect_codex,
            choose_codex,
            test_proxy,
            copy_launch_script,
            launch_codex,
            launch_codex_directly,
            verify_proxy_traffic,
            stop_codex,
            get_codex_status,
            open_logs
        ])
        .run(tauri::generate_context!())
        .expect("运行 Codex 代理启动器失败");
}
