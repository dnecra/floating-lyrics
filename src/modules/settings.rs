use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use tauri::Manager;

use crate::modules::mode::{current_mode, WindowMode};

// Settings structure
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppSettings {
    pub monitor_index: Option<usize>,
    pub click_through_enabled: bool,
    pub always_on_top_enabled: bool,
    pub disable_hover_hide: bool,
    pub word_bounce_disabled: bool,
    pub blur_enabled: bool,
    pub has_seen_welcome: bool,
    pub window_mode_x: Option<i32>,
    pub window_mode_y: Option<i32>,
    pub window_mode_width: Option<u32>,
    pub window_mode_height: Option<u32>,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            monitor_index: Some(0),
            click_through_enabled: true,
            always_on_top_enabled: true,
            disable_hover_hide: false,
            word_bounce_disabled: false,
            blur_enabled: true,
            has_seen_welcome: false,
            window_mode_x: None,
            window_mode_y: None,
            window_mode_width: None,
            window_mode_height: None,
        }
    }
}

impl AppSettings {
    pub fn default_for_mode(mode: WindowMode) -> Self {
        let mut settings = Self::default();
        if mode == WindowMode::Window {
            settings.always_on_top_enabled = false;
        }
        settings
    }
}

// Store selected monitor index
pub static SELECTED_MONITOR_INDEX: AtomicUsize = AtomicUsize::new(0);

// Store word bounce disabled state
pub static WORD_BOUNCE_DISABLED: AtomicBool = AtomicBool::new(false);
pub static BLUR_ENABLED: AtomicBool = AtomicBool::new(true);

pub static LYRICS_PAUSED: AtomicBool = AtomicBool::new(false);
pub static NORMAL_LYRICS_PAUSED: AtomicBool = AtomicBool::new(false);
pub static WINDOW_LYRICS_PAUSED: AtomicBool = AtomicBool::new(false);

// Track click-through state (default: enabled)
pub static CLICK_THROUGH_ENABLED: AtomicBool = AtomicBool::new(true);
pub static ALWAYS_ON_TOP_ENABLED: AtomicBool = AtomicBool::new(true);
pub static DISABLE_HOVER_HIDE: AtomicBool = AtomicBool::new(false);
pub static HAS_SEEN_WELCOME: AtomicBool = AtomicBool::new(false);

pub fn lyrics_paused_for_mode(mode: WindowMode) -> bool {
    match mode {
        WindowMode::Normal => NORMAL_LYRICS_PAUSED.load(std::sync::atomic::Ordering::SeqCst),
        WindowMode::Window => WINDOW_LYRICS_PAUSED.load(std::sync::atomic::Ordering::SeqCst),
    }
}

pub fn set_lyrics_paused_for_mode(mode: WindowMode, paused: bool) {
    match mode {
        WindowMode::Normal => {
            NORMAL_LYRICS_PAUSED.store(paused, std::sync::atomic::Ordering::SeqCst)
        }
        WindowMode::Window => {
            WINDOW_LYRICS_PAUSED.store(paused, std::sync::atomic::Ordering::SeqCst)
        }
    }
}

pub fn snapshot_settings() -> AppSettings {
    AppSettings {
        monitor_index: Some(SELECTED_MONITOR_INDEX.load(std::sync::atomic::Ordering::SeqCst)),
        click_through_enabled: CLICK_THROUGH_ENABLED.load(std::sync::atomic::Ordering::SeqCst),
        always_on_top_enabled: ALWAYS_ON_TOP_ENABLED.load(std::sync::atomic::Ordering::SeqCst),
        disable_hover_hide: DISABLE_HOVER_HIDE.load(std::sync::atomic::Ordering::SeqCst),
        word_bounce_disabled: WORD_BOUNCE_DISABLED.load(std::sync::atomic::Ordering::SeqCst),
        blur_enabled: BLUR_ENABLED.load(std::sync::atomic::Ordering::SeqCst),
        has_seen_welcome: HAS_SEEN_WELCOME.load(std::sync::atomic::Ordering::SeqCst),
        window_mode_x: None,
        window_mode_y: None,
        window_mode_width: None,
        window_mode_height: None,
    }
}

pub fn save_current_settings(app: &tauri::AppHandle) {
    let mode = current_mode();
    let mut snapshot = snapshot_settings();
    if mode == WindowMode::Window {
        let existing = load_settings_for_mode(app, WindowMode::Window);
        snapshot.window_mode_x = existing.window_mode_x;
        snapshot.window_mode_y = existing.window_mode_y;
        snapshot.window_mode_width = existing.window_mode_width;
        snapshot.window_mode_height = existing.window_mode_height;
    }
    save_settings_for_mode(app, &snapshot, mode);
}

pub fn apply_loaded_settings(settings: &AppSettings) {
    CLICK_THROUGH_ENABLED.store(
        settings.click_through_enabled,
        std::sync::atomic::Ordering::SeqCst,
    );
    ALWAYS_ON_TOP_ENABLED.store(
        settings.always_on_top_enabled,
        std::sync::atomic::Ordering::SeqCst,
    );
    DISABLE_HOVER_HIDE.store(
        settings.disable_hover_hide,
        std::sync::atomic::Ordering::SeqCst,
    );
    WORD_BOUNCE_DISABLED.store(
        settings.word_bounce_disabled,
        std::sync::atomic::Ordering::SeqCst,
    );
    BLUR_ENABLED.store(settings.blur_enabled, std::sync::atomic::Ordering::SeqCst);
    HAS_SEEN_WELCOME.store(
        settings.has_seen_welcome,
        std::sync::atomic::Ordering::SeqCst,
    );
    SELECTED_MONITOR_INDEX.store(
        settings.monitor_index.unwrap_or(0),
        std::sync::atomic::Ordering::SeqCst,
    );
}

// Settings file path
pub fn get_settings_path_for_mode(app: &tauri::AppHandle, mode: WindowMode) -> PathBuf {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .expect("Failed to get app data directory");
    fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");
    app_data_dir.join(mode.settings_file_name())
}

// Load settings from file
pub fn load_settings_for_mode(app: &tauri::AppHandle, mode: WindowMode) -> AppSettings {
    let settings_path = get_settings_path_for_mode(app, mode);

    if settings_path.exists() {
        match fs::read_to_string(&settings_path) {
            Ok(contents) => match serde_json::from_str::<AppSettings>(&contents) {
                Ok(settings) => {
                    println!("Loaded settings from {:?}", settings_path);
                    return settings;
                }
                Err(e) => {
                    println!("Failed to parse settings: {}, using defaults", e);
                }
            },
            Err(e) => {
                println!("Failed to read settings file: {}, using defaults", e);
            }
        }
    }

    AppSettings::default_for_mode(mode)
}
// Save settings to file
pub fn save_settings_for_mode(app: &tauri::AppHandle, settings: &AppSettings, mode: WindowMode) {
    let settings_path = get_settings_path_for_mode(app, mode);

    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            if let Err(e) = fs::write(&settings_path, json) {
                println!("Failed to save settings: {}", e);
            } else {
                println!("Saved settings to {:?}", settings_path);
            }
        }
        Err(e) => {
            println!("Failed to serialize settings: {}", e);
        }
    }
}

pub fn save_window_mode_bounds(app: &tauri::AppHandle, x: i32, y: i32, width: u32, height: u32) {
    let mut settings = load_settings_for_mode(app, WindowMode::Window);
    settings.window_mode_x = Some(x);
    settings.window_mode_y = Some(y);
    settings.window_mode_width = Some(width);
    settings.window_mode_height = Some(height);
    save_settings_for_mode(app, &settings, WindowMode::Window);
}
