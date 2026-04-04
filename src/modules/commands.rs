use crate::modules::mode;
use crate::modules::settings::*;
use std::sync::atomic::Ordering;

#[tauri::command]
pub fn set_click_through(app: tauri::AppHandle, enabled: bool) {
    crate::modules::click_through::set_click_through(&app, enabled);
}

#[tauri::command]
pub fn update_layout_container_bounds(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    viewport_width: f64,
    viewport_height: f64,
    exists: bool,
) {
    crate::modules::window::update_layout_container_bounds(
        x,
        y,
        width,
        height,
        viewport_width,
        viewport_height,
        exists,
    );
}

#[tauri::command]
pub fn toggle_window_mode_always_on_top(app: tauri::AppHandle) {
    if mode::current_mode() != mode::WindowMode::Window {
        return;
    }

    let new_state = !ALWAYS_ON_TOP_ENABLED.load(Ordering::SeqCst);
    ALWAYS_ON_TOP_ENABLED.store(new_state, Ordering::SeqCst);
    save_current_settings(&app);

    if let Some(window) = mode::active_window(&app) {
        crate::modules::window::apply_always_on_top_preference(&window);
    }

    crate::modules::menu::update_color_menu_labels(&app);
}

#[tauri::command]
pub fn set_blur_enabled(app: tauri::AppHandle, enabled: bool) {
    BLUR_ENABLED.store(enabled, Ordering::SeqCst);
    save_current_settings(&app);

    if let Some(window) = mode::active_window(&app) {
        crate::modules::scripts::apply_blur_enabled(&window, enabled);
    }

    crate::modules::menu::update_color_menu_labels(&app);
}

#[tauri::command]
pub fn toggle_window_mode_fullscreen(app: tauri::AppHandle) {
    let Some(window) = mode::active_window(&app) else {
        return;
    };
    if mode::current_mode() != mode::WindowMode::Window {
        return;
    }

    let is_fullscreen = window.is_fullscreen().unwrap_or(false);
    let _ = window.set_fullscreen(!is_fullscreen);
    let _ = window.set_focus();
    let _ = window.eval("window.focus(); try { document.body && document.body.focus({ preventScroll: true }); } catch (_) {}");
}

#[tauri::command]
pub fn minimize_window_mode(app: tauri::AppHandle) {
    let Some(window) = mode::active_window(&app) else {
        return;
    };
    if mode::current_mode() != mode::WindowMode::Window {
        return;
    }

    let _ = window.minimize();
}

#[tauri::command]
pub fn close_window_mode(app: tauri::AppHandle) {
    crate::app_runtime::close_window_mode(&app);
}

#[tauri::command]
pub fn start_window_mode_dragging(app: tauri::AppHandle) {
    let Some(window) = mode::active_window(&app) else {
        return;
    };
    if mode::current_mode() != mode::WindowMode::Window {
        return;
    }

    let _ = window.set_focus();
    let _ = window.start_dragging();
}

#[tauri::command]
pub fn log_hover_probe(source: String, event: String, x: f64, y: f64, target: String) {
    println!(
        "[hover-probe] source={} event={} x={:.1} y={:.1} target={}",
        source, event, x, y, target
    );
}

#[tauri::command]
pub fn get_window_mode_chrome_state(app: tauri::AppHandle) -> (bool, bool) {
    let Some(window) = mode::active_window(&app) else {
        return (false, false);
    };
    (
        window.is_always_on_top().unwrap_or(false),
        window.is_fullscreen().unwrap_or(false),
    )
}
