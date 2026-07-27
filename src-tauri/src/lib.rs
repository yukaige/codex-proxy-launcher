mod commands;
mod core;
mod detector;
mod launcher;
mod logger;
mod proxy;
mod store;
mod traffic;
mod types;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    commands::run();
}
