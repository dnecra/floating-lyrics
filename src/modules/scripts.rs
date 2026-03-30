use crate::modules::mode;
use crate::modules::mode::WindowMode;
use crate::modules::settings::*;
use std::sync::atomic::Ordering;

const TOGGLE_FANCY_ANIMATION_SCRIPT: &str =
    include_str!("../../scripts/toggle-expensive-effects.js");
const TOGGLE_BLUR_EFFECTS_SCRIPT: &str = include_str!("../../scripts/toggle-blur-effects.js");
const WINDOW_MODE_CHROME_SCRIPT: &str = include_str!("../../scripts/window-mode-chrome.js");

pub struct Scripts {
    pub transparent_bg_script: &'static str,
    pub layout_hover_script: &'static str,
    pub close_window_script: &'static str,
}

pub fn apply_blur_enabled(window: &tauri::WebviewWindow, enabled: bool) {
    let _ = window.eval(TOGGLE_BLUR_EFFECTS_SCRIPT);
    let _ = window.eval(&format!(
        "if (window.setBlurEffectsEnabled) {{ window.setBlurEffectsEnabled({}); }}",
        if enabled { "true" } else { "false" }
    ));
}

pub fn apply_lyrics_paused(window: &tauri::WebviewWindow, paused: bool) {
    let desired = if paused { "true" } else { "false" };
    let _ = window.eval(&format!(
        r#"
        (() => {{
            const desired = {desired};
            if (window.__floatingLyricsPaused === desired) {{
                return;
            }}
            if (window.togglePause) {{
                try {{
                    window.togglePause();
                    window.__floatingLyricsPaused = desired;
                }} catch (_) {{}}
                return;
            }}
            window.__floatingLyricsPaused = desired;
        }})();
        "#
    ));
}

// Apply fancy animation disabled style
pub fn apply_fancy_animation_disabled(window: &tauri::WebviewWindow) {
    let _ = window.eval(TOGGLE_FANCY_ANIMATION_SCRIPT);
    let _ = window
        .eval("if (window.setFancyAnimationDisabled) { window.setFancyAnimationDisabled(true); }");
}

// Toggle fancy animation disabled state
pub fn toggle_fancy_animation_disabled(app: tauri::AppHandle) {
    let current_state = WORD_BOUNCE_DISABLED.load(Ordering::SeqCst);
    let new_state = !current_state;

    WORD_BOUNCE_DISABLED.store(new_state, Ordering::SeqCst);

    if let Some(window) = mode::active_window(&app) {
        if new_state {
            apply_fancy_animation_disabled(&window);
        } else {
            let _ = window.eval(TOGGLE_FANCY_ANIMATION_SCRIPT);
            let _ = window.eval("if (window.setFancyAnimationDisabled) { window.setFancyAnimationDisabled(false); }");
        }
    }

    crate::modules::settings::save_current_settings(&app);

    // Update menu labels
    crate::modules::menu::update_color_menu_labels(&app);
}

pub fn toggle_blur_enabled(app: tauri::AppHandle) {
    let current_state = BLUR_ENABLED.load(Ordering::SeqCst);
    let new_state = !current_state;

    BLUR_ENABLED.store(new_state, Ordering::SeqCst);

    if let Some(window) = mode::active_window(&app) {
        apply_blur_enabled(&window, new_state);
    }

    crate::modules::settings::save_current_settings(&app);
    crate::modules::menu::update_color_menu_labels(&app);
}

// Inject all scripts rapidly during initial page load
pub fn inject_scripts_rapidly(
    window: tauri::WebviewWindow,
    scripts: &'static Scripts,
    iterations: u32,
    mode: WindowMode,
) {
    std::thread::spawn(move || {
        // One-time init scripts. They install helpers/observers and should not be duplicated.
        match mode {
            WindowMode::Normal => {
                let _ = window.eval(scripts.transparent_bg_script);
                let _ = window.eval(scripts.layout_hover_script);
                let _ = window.eval(scripts.close_window_script);
            }
            WindowMode::Window => {
                let _ = window.eval(WINDOW_MODE_CHROME_SCRIPT);
            }
        }

        for _ in 0..iterations {
            if mode == WindowMode::Normal {
                let _ = window.eval(
                    "if (window.__pushLayoutHoverBounds) { try { window.__pushLayoutHoverBounds(); } catch (_) {} }",
                );
            }
            apply_blur_enabled(&window, BLUR_ENABLED.load(Ordering::SeqCst));
            apply_lyrics_paused(&window, LYRICS_PAUSED.load(Ordering::SeqCst));

            if WORD_BOUNCE_DISABLED.load(Ordering::SeqCst) {
                apply_fancy_animation_disabled(&window);
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}
