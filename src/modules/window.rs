use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::modules::mode::{self, WindowMode};
use crate::modules::settings::*;

// ── Window position (physical pixels) ────────────────────────────────────────
pub static WINDOW_X: AtomicI32 = AtomicI32::new(0);
pub static WINDOW_Y: AtomicI32 = AtomicI32::new(0);

// ── Layout container bounds (scaled by BOUNDS_SCALE for integer atomics) ──────
const BOUNDS_SCALE: f64 = 100.0;
const HOVER_AUTO_DISABLE_THRESHOLD: f64 = 0.35;
static LAYOUT_EXISTS: AtomicBool = AtomicBool::new(false);
static LAYOUT_LEFT: AtomicI32 = AtomicI32::new(0);
static LAYOUT_TOP: AtomicI32 = AtomicI32::new(0);
static LAYOUT_WIDTH: AtomicI32 = AtomicI32::new(0);
static LAYOUT_HEIGHT: AtomicI32 = AtomicI32::new(0);
static HOVER_HIDE_AUTO_DISABLED: AtomicBool = AtomicBool::new(false);

// ── Hover-hide state ──────────────────────────────────────────────────────────
const HOVER_SHOW_DELAY: Duration = Duration::from_millis(500);
const FADE_STEPS: u32 = 8;
const FADE_STEP_MS: u64 = 22;
const MODE_FADE_STEPS: u32 = 12;
const MODE_FADE_STEP_MS: u64 = 25;

static WINDOW_HIDDEN_BY_HOVER: AtomicBool = AtomicBool::new(false);

// ── Welcome-mode flag (public – read by hotkey guard in app_runtime) ─────────
/// True while the window is displaying a /welcome or /guide URL.
/// In this mode click-through and hover-hide are fully suspended.
pub static WELCOME_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);

// ── Interaction-override (used by click-through module) ───────────────────────
static INTERACTION_OVERRIDE_ACTIVE: AtomicBool = AtomicBool::new(false);

// ─────────────────────────────────────────────────────────────────────────────
// Welcome mode  –  enter / exit
// ─────────────────────────────────────────────────────────────────────────────

/// Called whenever the window navigates to a /welcome or /guide URL.
/// Disables click-through and hover-hide so the user can interact with the page.
pub fn enter_welcome_mode(window: &tauri::WebviewWindow) {
    WELCOME_MODE_ACTIVE.store(true, Ordering::SeqCst);
    WINDOW_HIDDEN_BY_HOVER.store(false, Ordering::SeqCst);
    INTERACTION_OVERRIDE_ACTIVE.store(false, Ordering::SeqCst);

    // Make window fully interactive.
    let _ = window.set_ignore_cursor_events(false);
    let _ = window.set_focusable(true);

    // Ensure it's visible and on top.
    force_show_immediate(window);
}

/// Called when the window navigates away from /welcome → /lyrics.
/// Restores the persisted click-through and hover-hide behaviour.
pub fn exit_welcome_mode(window: &tauri::WebviewWindow) {
    WELCOME_MODE_ACTIVE.store(false, Ordering::SeqCst);
    WINDOW_HIDDEN_BY_HOVER.store(false, Ordering::SeqCst);
    INTERACTION_OVERRIDE_ACTIVE.store(false, Ordering::SeqCst);

    // Restore click-through from persisted setting.
    let ct = CLICK_THROUGH_ENABLED.load(Ordering::SeqCst);
    let _ = window.set_ignore_cursor_events(ct);
    let _ = window.set_focusable(!ct);

    apply_always_on_top_preference(window);
}

// ─────────────────────────────────────────────────────────────────────────────
// Settings apply
// ─────────────────────────────────────────────────────────────────────────────
pub fn apply_settings(
    app: &tauri::AppHandle,
    settings: &AppSettings,
    scripts: &crate::modules::scripts::Scripts,
) {
    apply_loaded_settings(settings);

    if let Some(window) = mode::active_window(app) {
        apply_settings_to_window(app, &window, settings, scripts, mode::current_mode());
    }

    if mode::current_mode() == WindowMode::Normal && settings.monitor_index.is_some() {
        if let Some(window) = mode::get_window(app, WindowMode::Normal) {
            setup_window_position(app, &window);
        }
    }

    crate::modules::menu::update_color_menu_labels(app);
}

pub fn apply_settings_to_window(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    settings: &AppSettings,
    scripts: &crate::modules::scripts::Scripts,
    mode: WindowMode,
) {
    match mode {
        WindowMode::Normal => {
            if !WELCOME_MODE_ACTIVE.load(Ordering::SeqCst) {
                let _ = window.set_ignore_cursor_events(settings.click_through_enabled);
                let _ = window.set_focusable(!settings.click_through_enabled);
            }
            let _ = window.eval(scripts.layout_hover_script);
            let _ = window.eval(scripts.close_window_script);
        }
        WindowMode::Window => {
            let _ = window.set_ignore_cursor_events(false);
            let _ = window.set_focusable(true);
        }
    }

    crate::modules::scripts::apply_blur_enabled(window, settings.blur_enabled);

    if settings.word_bounce_disabled {
        crate::modules::scripts::apply_fancy_animation_disabled(window);
    }

    if mode == WindowMode::Normal && settings.monitor_index.is_some() {
        setup_window_position(app, window);
    }

    apply_always_on_top_preference(window);
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout container bounds  (called from Tauri command)
// ─────────────────────────────────────────────────────────────────────────────
pub fn update_layout_container_bounds(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    viewport_width: f64,
    viewport_height: f64,
    exists: bool,
) {
    if !exists
        || !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
    {
        LAYOUT_EXISTS.store(false, Ordering::SeqCst);
        set_hover_hide_auto_disabled(false);
        return;
    }
    LAYOUT_LEFT.store((x * BOUNDS_SCALE) as i32, Ordering::SeqCst);
    LAYOUT_TOP.store((y * BOUNDS_SCALE) as i32, Ordering::SeqCst);
    LAYOUT_WIDTH.store((width.max(0.0) * BOUNDS_SCALE) as i32, Ordering::SeqCst);
    LAYOUT_HEIGHT.store((height.max(0.0) * BOUNDS_SCALE) as i32, Ordering::SeqCst);
    LAYOUT_EXISTS.store(true, Ordering::SeqCst);

    let should_auto_disable = viewport_width.is_finite()
        && viewport_height.is_finite()
        && viewport_width > 0.0
        && viewport_height > 0.0
        && ((width.max(0.0) * height.max(0.0)) / (viewport_width * viewport_height))
            >= HOVER_AUTO_DISABLE_THRESHOLD;
    set_hover_hide_auto_disabled(should_auto_disable);
}

fn set_hover_hide_auto_disabled(disabled: bool) {
    let previous = HOVER_HIDE_AUTO_DISABLED.swap(disabled, Ordering::SeqCst);
    if previous != disabled {
        crate::modules::menu::refresh_menu_labels();
    }
}

pub fn is_hover_hide_effectively_disabled() -> bool {
    DISABLE_HOVER_HIDE.load(Ordering::SeqCst) || HOVER_HIDE_AUTO_DISABLED.load(Ordering::SeqCst)
}

pub fn is_hover_hide_auto_disabled() -> bool {
    HOVER_HIDE_AUTO_DISABLED.load(Ordering::SeqCst)
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout hover controller  (background thread)
// ─────────────────────────────────────────────────────────────────────────────
pub fn start_layout_hover_controller(window: tauri::WebviewWindow) {
    thread::spawn(move || {
        let mut pending_show_at: Option<Instant> = None;

        loop {
            thread::sleep(Duration::from_millis(90));

            if mode::current_mode() != WindowMode::Normal {
                pending_show_at = None;
                WINDOW_HIDDEN_BY_HOVER.store(false, Ordering::SeqCst);
                continue;
            }

            // Skip entirely in welcome mode.
            if WELCOME_MODE_ACTIVE.load(Ordering::SeqCst) {
                pending_show_at = None;
                if WINDOW_HIDDEN_BY_HOVER.swap(false, Ordering::SeqCst) {
                    force_show_immediate(&window);
                }
                continue;
            }

            let window_visible = window.is_visible().unwrap_or(true);
            let hidden_by_hover = WINDOW_HIDDEN_BY_HOVER.load(Ordering::SeqCst);
            let hover_active =
                !LYRICS_PAUSED.load(Ordering::SeqCst) && (window_visible || hidden_by_hover);

            if !hover_active {
                pending_show_at = None;
                WINDOW_HIDDEN_BY_HOVER.store(false, Ordering::SeqCst);
                continue;
            }

            // Interaction override (Alt+Shift+F held) → always show.
            if INTERACTION_OVERRIDE_ACTIVE.load(Ordering::SeqCst) {
                pending_show_at = None;
                if WINDOW_HIDDEN_BY_HOVER.swap(false, Ordering::SeqCst) {
                    force_show_immediate(&window);
                }
                continue;
            }

            // Click-through disabled → user is in drag/resize mode → show.
            if !CLICK_THROUGH_ENABLED.load(Ordering::SeqCst) {
                pending_show_at = None;
                if WINDOW_HIDDEN_BY_HOVER.swap(false, Ordering::SeqCst) {
                    fade_show(&window);
                }
                continue;
            }

            // Hover-hide disabled via tray setting.
            if is_hover_hide_effectively_disabled() {
                pending_show_at = None;
                if WINDOW_HIDDEN_BY_HOVER.swap(false, Ordering::SeqCst) {
                    fade_show(&window);
                }
                continue;
            }

            let inside = is_cursor_inside_layout_container(&window);
            if inside {
                pending_show_at = None;
                if !WINDOW_HIDDEN_BY_HOVER.load(Ordering::SeqCst) {
                    fade_hide(&window);
                    WINDOW_HIDDEN_BY_HOVER.store(true, Ordering::SeqCst);
                }
                continue;
            }

            // Cursor outside → schedule show after delay.
            if WINDOW_HIDDEN_BY_HOVER.load(Ordering::SeqCst) {
                if pending_show_at.is_none() {
                    pending_show_at = Some(Instant::now() + HOVER_SHOW_DELAY);
                }
                if let Some(deadline) = pending_show_at {
                    if Instant::now() >= deadline {
                        fade_show(&window);
                        WINDOW_HIDDEN_BY_HOVER.store(false, Ordering::SeqCst);
                        pending_show_at = None;
                    }
                }
            } else {
                pending_show_at = None;
            }
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Interaction override  (used by click_through module)
// ─────────────────────────────────────────────────────────────────────────────
pub fn set_interaction_override(window: &tauri::WebviewWindow, active: bool) {
    INTERACTION_OVERRIDE_ACTIVE.store(active, Ordering::SeqCst);
    if active {
        force_show_immediate(window);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Visibility helpers
// ─────────────────────────────────────────────────────────────────────────────
pub fn force_show_immediate(window: &tauri::WebviewWindow) {
    WINDOW_HIDDEN_BY_HOVER.store(false, Ordering::SeqCst);
    set_window_alpha(window, 255);
    if !window.is_visible().unwrap_or(true) {
        show_without_focus(window);
    }
    apply_always_on_top_preference(window);
}

pub fn show_and_focus_immediate(window: &tauri::WebviewWindow) {
    WINDOW_HIDDEN_BY_HOVER.store(false, Ordering::SeqCst);
    set_window_alpha(window, 255);
    if !window.is_visible().unwrap_or(false) {
        show_without_focus(window);
    }
    let _ = window.set_focus();
    apply_always_on_top_preference(window);
}

pub fn animate_show_and_focus(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        WINDOW_HIDDEN_BY_HOVER.store(false, Ordering::SeqCst);
        let _ = window.unminimize();
        set_window_alpha(window, 0);
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        let _ = window.set_focus();
        apply_always_on_top_preference(window);
        for step in 0..=MODE_FADE_STEPS {
            let alpha = ((step as f64 / MODE_FADE_STEPS as f64) * 255.0).round() as u8;
            set_window_alpha(window, alpha);
            thread::sleep(Duration::from_millis(MODE_FADE_STEP_MS));
        }
        set_window_alpha(window, 255);
    }

    #[cfg(not(target_os = "windows"))]
    {
        show_and_focus_immediate(window);
    }
}

pub fn animate_hide(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        for step in (0..=MODE_FADE_STEPS).rev() {
            let alpha = ((step as f64 / MODE_FADE_STEPS as f64) * 255.0).round() as u8;
            set_window_alpha(window, alpha);
            thread::sleep(Duration::from_millis(MODE_FADE_STEP_MS));
        }
        let _ = window.hide();
        set_window_alpha(window, 255);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = window.hide();
    }
}

#[cfg(target_os = "windows")]
pub fn show_without_focus(window: &tauri::WebviewWindow) {
    if window.is_visible().unwrap_or(false) {
        return;
    }
    set_window_alpha(window, 0);
    let _ = window.show();
    thread::sleep(Duration::from_millis(16));
    set_window_alpha(window, 255);
}

#[cfg(not(target_os = "windows"))]
pub fn show_without_focus(window: &tauri::WebviewWindow) {
    let _ = window.show();
}

// ─────────────────────────────────────────────────────────────────────────────
// Always-on-top  –  simple and passive (no polling, no fullscreen detection)
//
// Design goals:
//   • When the setting is ON → window stays on top.
//   • When the setting is OFF → window behaves normally.
//   • We NEVER poll in a background thread → no taskbar flicker, no game input
//     interference.
//   • We re-apply on natural Tauri window events (focus, move, resize).
//   • We do NOT use HWND_TOPMOST in a tight loop.
// ─────────────────────────────────────────────────────────────────────────────
pub fn apply_always_on_top_preference(window: &tauri::WebviewWindow) {
    // In welcome mode we don't force always-on-top so the user can interact
    // with the page naturally (e.g. browser focus is fine).
    let want_topmost = ALWAYS_ON_TOP_ENABLED.load(Ordering::SeqCst)
        && !LYRICS_PAUSED.load(Ordering::SeqCst)
        && window.is_visible().unwrap_or(true);

    // Only call the OS if the state actually needs to change.
    let currently = window.is_always_on_top().unwrap_or(!want_topmost);
    if currently != want_topmost {
        let _ = window.set_always_on_top(want_topmost);
    }

    // On Windows, back up the Tauri call with a direct WinAPI call.
    // We do this ONCE per preference change, NOT in a loop.
    #[cfg(target_os = "windows")]
    if want_topmost {
        set_hwnd_topmost(window);
    }
}

pub fn enforce_topmost(window: &tauri::WebviewWindow) {
    apply_always_on_top_preference(window);
}

// ─────────────────────────────────────────────────────────────────────────────
// Window position / monitor tracking
// ─────────────────────────────────────────────────────────────────────────────
pub fn setup_window_position(app: &tauri::AppHandle, window: &tauri::WebviewWindow) {
    let selected_idx = SELECTED_MONITOR_INDEX.load(Ordering::SeqCst);
    if let Ok(monitors) = app.available_monitors() {
        if let Some(monitor) = monitors.get(selected_idx) {
            apply_monitor_layout(
                window,
                monitor.position().x,
                monitor.position().y,
                *monitor.size(),
            );
            return;
        }
    }
    // Fallback: current monitor.
    if let Ok(Some(monitor)) = window.current_monitor() {
        apply_monitor_layout(window, 0, 0, *monitor.size());
    }
}

fn apply_monitor_layout(
    window: &tauri::WebviewWindow,
    monitor_x: i32,
    monitor_y: i32,
    size: tauri::PhysicalSize<u32>,
) {
    #[cfg(target_os = "windows")]
    let taskbar_height = get_monitor_taskbar_height(monitor_x, monitor_y, size);
    #[cfg(not(target_os = "windows"))]
    let taskbar_height = 0u32;

    let x = monitor_x;
    let y = monitor_y - taskbar_height as i32;

    WINDOW_X.store(x, Ordering::SeqCst);
    WINDOW_Y.store(y, Ordering::SeqCst);

    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: size.width,
        height: size.height,
    }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
    let _ = window.set_fullscreen(false);
    apply_always_on_top_preference(window);
}

pub fn start_monitor_watcher(window: tauri::WebviewWindow) {
    thread::spawn(move || {
        let mut last_key: Option<String> = None;
        loop {
            thread::sleep(Duration::from_millis(1500));
            if let Ok(Some(mon)) = window.current_monitor() {
                let key = format!(
                    "{}:{}:{}x{}",
                    mon.position().x,
                    mon.position().y,
                    mon.size().width,
                    mon.size().height
                );
                if last_key.as_ref() != Some(&key) {
                    last_key = Some(key);
                    apply_monitor_layout(&window, mon.position().x, mon.position().y, *mon.size());
                    apply_always_on_top_preference(&window);
                    #[cfg(target_os = "macos")]
                    let _ = window.set_visible_on_all_workspaces(true);
                }
            }
        }
    });
}

pub fn setup_window_events(window: &tauri::WebviewWindow) {
    let w = window.clone();
    window.on_window_event(move |event| {
        match event {
            tauri::WindowEvent::Focused(_)
            | tauri::WindowEvent::Moved(_)
            | tauri::WindowEvent::Resized(_) => {
                // Re-apply on natural events.  This is passive – only fires
                // when the OS tells us something changed, so it cannot cause
                // the flicker / focus-steal issues that polling would.
                apply_always_on_top_preference(&w);
            }
            _ => {}
        }
    });
}

pub fn setup_window_mode_state_tracking(app: tauri::AppHandle, window: &tauri::WebviewWindow) {
    let w = window.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
            let Ok(pos) = w.outer_position() else { return };
            let Ok(size) = w.inner_size() else { return };
            crate::modules::settings::save_window_mode_bounds(
                &app,
                pos.x,
                pos.y,
                size.width,
                size.height,
            );
        }
        _ => {}
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Fade helpers
// ─────────────────────────────────────────────────────────────────────────────
fn fade_hide(window: &tauri::WebviewWindow) {
    for step in (0..=FADE_STEPS).rev() {
        let alpha = ((step as f64 / FADE_STEPS as f64) * 255.0).round() as u8;
        set_window_alpha(window, alpha);
        thread::sleep(Duration::from_millis(FADE_STEP_MS));
    }
    #[cfg(not(target_os = "windows"))]
    let _ = window.hide();
}

fn fade_show(window: &tauri::WebviewWindow) {
    set_window_alpha(window, 0);
    #[cfg(target_os = "windows")]
    {
        if !window.is_visible().unwrap_or(true) {
            show_without_focus(window);
        }
    }
    #[cfg(not(target_os = "windows"))]
    show_without_focus(window);
    apply_always_on_top_preference(window);
    for step in 0..=FADE_STEPS {
        let alpha = ((step as f64 / FADE_STEPS as f64) * 255.0).round() as u8;
        set_window_alpha(window, alpha);
        thread::sleep(Duration::from_millis(FADE_STEP_MS));
    }
    set_window_alpha(window, 255);
}

// ─────────────────────────────────────────────────────────────────────────────
// Cursor / layout hit-test (Windows only)
// ─────────────────────────────────────────────────────────────────────────────
fn is_cursor_inside_layout_container(window: &tauri::WebviewWindow) -> bool {
    if !LAYOUT_EXISTS.load(Ordering::SeqCst) {
        return false;
    }

    #[cfg(target_os = "windows")]
    {
        let Some((cx, cy)) = get_cursor_pos_screen() else { return false };
        let pos = window.inner_position().or_else(|_| window.outer_position());
        let Ok(pos) = pos else { return false };
        let scale = window.scale_factor().unwrap_or(1.0);
        let lx = (cx - pos.x) as f64 / scale;
        let ly = (cy - pos.y) as f64 / scale;

        let left = LAYOUT_LEFT.load(Ordering::SeqCst) as f64 / BOUNDS_SCALE;
        let top = LAYOUT_TOP.load(Ordering::SeqCst) as f64 / BOUNDS_SCALE;
        let w = LAYOUT_WIDTH.load(Ordering::SeqCst) as f64 / BOUNDS_SCALE;
        let h = LAYOUT_HEIGHT.load(Ordering::SeqCst) as f64 / BOUNDS_SCALE;

        return lx >= left && lx <= left + w && ly >= top && ly <= top + h;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Windows-specific platform helpers
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
fn get_cursor_pos_screen() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    if unsafe { GetCursorPos(&mut p) }.is_ok() {
        Some((p.x, p.y))
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn get_monitor_taskbar_height(
    monitor_x: i32,
    monitor_y: i32,
    size: tauri::PhysicalSize<u32>,
) -> u32 {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    let cx = monitor_x.saturating_add((size.width / 2) as i32);
    let cy = monitor_y.saturating_add((size.height / 2) as i32);
    let mon = unsafe { MonitorFromPoint(POINT { x: cx, y: cy }, MONITOR_DEFAULTTONEAREST) };
    if mon.0.is_null() { return 0; }
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if !unsafe { GetMonitorInfoW(mon, &mut info as *mut MONITORINFO) }.as_bool() { return 0; }
    let bottom = (info.rcMonitor.bottom - info.rcWork.bottom).max(0) as u32;
    let top = (info.rcWork.top - info.rcMonitor.top).max(0) as u32;
    bottom.max(top)
}

/// One-shot HWND_TOPMOST call.  Called only when the preference is applied,
/// never in a polling loop.
#[cfg(target_os = "windows")]
fn set_hwnd_topmost(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let hwnd = HWND(hwnd.0 as *mut core::ffi::c_void);
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

#[cfg(target_os = "windows")]
fn set_window_alpha(window: &tauri::WebviewWindow, alpha: u8) {
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetLayeredWindowAttributes, SetWindowLongW,
        GWL_EXSTYLE, LWA_ALPHA, WS_EX_LAYERED,
    };
    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let hwnd = HWND(hwnd.0 as *mut core::ffi::c_void);
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
            let layered = WS_EX_LAYERED.0 as i32;
            if ex & layered == 0 {
                let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, ex | layered);
            }
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn set_window_alpha(_window: &tauri::WebviewWindow, _alpha: u8) {}

#[cfg(target_os = "windows")]
pub fn apply_windows_visual_tweaks(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::{BOOL, COLORREF, HWND};
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_TRANSITIONS_FORCEDISABLED,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWM_WINDOW_CORNER_PREFERENCE,
    };
    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let hwnd = HWND(hwnd.0 as *mut core::ffi::c_void);
            let corner = DWM_WINDOW_CORNER_PREFERENCE(DWMWCP_DONOTROUND.0);
            let _ = DwmSetWindowAttribute(
                hwnd, DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            );
            let no_border = COLORREF(0xFF_FF_FF_FE);
            let _ = DwmSetWindowAttribute(
                hwnd, DWMWA_BORDER_COLOR,
                &no_border as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<COLORREF>() as u32,
            );
            let disable = BOOL(1);
            let _ = DwmSetWindowAttribute(
                hwnd, DWMWA_TRANSITIONS_FORCEDISABLED,
                &disable as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<BOOL>() as u32,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_windows_visual_tweaks(_window: &tauri::WebviewWindow) {}
