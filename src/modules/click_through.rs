use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter};

use crate::modules::mode::{self, WindowMode};
use crate::modules::settings::{save_current_settings, CLICK_THROUGH_ENABLED};
use crate::modules::window::WELCOME_MODE_ACTIVE;

pub fn set_click_through(app: &AppHandle, enabled: bool) {
    set_click_through_inner(app, enabled, true);
}

fn set_click_through_inner(app: &AppHandle, enabled: bool, persist: bool) {
    // Never apply click-through while the welcome page is active.
    if WELCOME_MODE_ACTIVE.load(Ordering::SeqCst) {
        // Still persist the intent so it takes effect when leaving welcome.
        if persist {
            CLICK_THROUGH_ENABLED.store(enabled, Ordering::SeqCst);
            save_current_settings(app);
        }
        return;
    }

    let current = CLICK_THROUGH_ENABLED.load(Ordering::SeqCst);

    // Always sync the OS window state even if the flag hasn't changed
    // (e.g. first call after leaving welcome mode).
    if let Some(window) = mode::get_window(app, WindowMode::Normal) {
        if current != enabled {
            CLICK_THROUGH_ENABLED.store(enabled, Ordering::SeqCst);
        }

        let _ = window.set_ignore_cursor_events(enabled);
        let _ = window.set_focusable(!enabled);

        if !enabled {
            crate::modules::window::set_interaction_override(&window, true);
            request_layout_bounds_sync(&window);
            force_show_width_control(&window);
        } else {
            crate::modules::window::set_interaction_override(&window, false);
            request_layout_bounds_sync(&window);
            force_hide_width_control(&window);
        }

        if persist && current != enabled && mode::current_mode() == WindowMode::Normal {
            save_current_settings(app);
        }

        // Sync tray menu label.
        if let Some(item) = app.menu().and_then(|m| m.get("click_through")) {
            if let Some(mi) = item.as_menuitem() {
                let label = if enabled {
                    "[x] Click-through"
                } else {
                    "[ ] Click-through"
                };
                let _ = mi.set_text(label);
            }
        }

        let _ = app.emit("click-through-changed", enabled);
    }
}

#[allow(dead_code)]
pub fn toggle_click_through(app: &AppHandle) {
    let enabled = !CLICK_THROUGH_ENABLED.load(Ordering::SeqCst);
    set_click_through(app, enabled);
}

pub fn set_click_through_runtime_no_persist(app: &AppHandle, enabled: bool) {
    set_click_through_inner(app, enabled, false);
}

// ─────────────────────────────────────────────────────────────────────────────
// Width-control / close-control visibility helpers
// ─────────────────────────────────────────────────────────────────────────────

fn force_show_width_control(window: &tauri::WebviewWindow) {
    let _ = window.eval(
        r#"(function () {
            const STYLE_ID = '__force_show_lyrics_width_control';
            let style = document.getElementById(STYLE_ID);
            if (!style) {
                style = document.createElement('style');
                style.id = STYLE_ID;
                (document.head || document.documentElement).appendChild(style);
            }
            style.textContent = `
                #lyrics-width-control,
                .lyrics-width-control,
                [id*="lyrics-width-control"],
                [class*="lyrics-width-control"] {
                    display: block !important;
                    visibility: visible !important;
                    opacity: 1 !important;
                    pointer-events: auto !important;
                }
            `;
            const wc = document.getElementById('lyrics-width-control');
            if (wc) wc.classList.add('show');
        })();"#,
    );
}

fn force_hide_width_control(window: &tauri::WebviewWindow) {
    let _ = window.eval(
        r#"(function () {
            const STYLE_ID = '__force_show_lyrics_width_control';
            const style = document.getElementById(STYLE_ID);
            if (style && style.parentNode) { style.parentNode.removeChild(style); }
            const wc = document.getElementById('lyrics-width-control');
            if (wc) wc.classList.remove('show');
        })();"#,
    );
}

fn request_layout_bounds_sync(window: &tauri::WebviewWindow) {
    let _ = window.eval(
        r#"(function () {
            if (window.__pushLayoutHoverBounds) {
                try { window.__pushLayoutHoverBounds(); } catch (_) {}
            }
        })();"#,
    );
}
