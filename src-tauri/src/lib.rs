mod commands;
mod core;
mod detector;
mod launcher;
mod logger;
mod proxy;
mod store;
mod traffic;
mod types;
#[cfg(target_os = "windows")]
mod windows_dotenv;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    commands::run();
}
