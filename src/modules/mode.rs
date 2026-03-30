use std::sync::atomic::{AtomicU8, Ordering};
use tauri::Manager;

pub const NORMAL_WINDOW_LABEL: &str = "main";
pub const WINDOW_MODE_LABEL: &str = "window_mode";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMode {
    Normal,
    Window,
}

impl WindowMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => NORMAL_WINDOW_LABEL,
            Self::Window => WINDOW_MODE_LABEL,
        }
    }

    pub fn settings_file_name(self) -> &'static str {
        match self {
            Self::Normal => "settings.normal.json",
            Self::Window => "settings.window.json",
        }
    }
}

static CURRENT_MODE: AtomicU8 = AtomicU8::new(0);

pub fn current_mode() -> WindowMode {
    match CURRENT_MODE.load(Ordering::SeqCst) {
        1 => WindowMode::Window,
        _ => WindowMode::Normal,
    }
}

pub fn set_current_mode(mode: WindowMode) {
    let raw = match mode {
        WindowMode::Normal => 0,
        WindowMode::Window => 1,
    };
    CURRENT_MODE.store(raw, Ordering::SeqCst);
}

pub fn get_window(app: &tauri::AppHandle, mode: WindowMode) -> Option<tauri::WebviewWindow> {
    app.get_webview_window(mode.label())
}

pub fn active_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    get_window(app, current_mode())
}
