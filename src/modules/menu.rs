use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri_plugin_opener::OpenerExt;

use crate::modules::mode::{self, WindowMode};
use crate::modules::scripts;
use crate::modules::settings::*;
use crate::modules::window::*;

lazy_static::lazy_static! {
    static ref TRAY_MENU_HANDLE: Mutex<Option<Menu<tauri::Wry>>> = Mutex::new(None);
    static ref SERVER_UPDATE_MENU_ITEM: Mutex<Option<MenuItem<tauri::Wry>>> = Mutex::new(None);
    static ref SERVER_UPDATE_SEPARATOR: Mutex<Option<PredefinedMenuItem<tauri::Wry>>> = Mutex::new(None);
}

pub fn set_runtime_tray_menu(menu: Menu<tauri::Wry>) {
    if let Ok(mut slot) = TRAY_MENU_HANDLE.lock() {
        *slot = Some(menu);
    }
}

fn active_menu(app: &tauri::AppHandle) -> Option<Menu<tauri::Wry>> {
    app.menu().or_else(|| {
        TRAY_MENU_HANDLE
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().cloned())
    })
}

fn normal_mode_only_controls_enabled() -> bool {
    mode::current_mode() == WindowMode::Normal
}

fn detect_local_ipv4() -> Ipv4Addr {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            let _ = socket.connect("8.8.8.8:80");
            socket.local_addr()
        })
        .ok()
        .and_then(|addr| match addr.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
            _ => None,
        })
        .unwrap_or(Ipv4Addr::new(127, 0, 0, 1))
}

fn local_ipv4_url() -> String {
    format!("http://{}:1312", detect_local_ipv4())
}

fn local_ipv4_menu_text() -> String {
    format!("{}:1312", detect_local_ipv4())
}

pub fn refresh_menu_labels() {
    if !normal_mode_only_controls_enabled() {
        return;
    }

    if let Ok(slot) = TRAY_MENU_HANDLE.lock() {
        if let Some(menu) = slot.as_ref() {
            if let Some(item) = menu.get("disable_hover_hide") {
                if let Some(check_item) = item.as_check_menuitem() {
                    let disabled = crate::modules::window::is_hover_hide_effectively_disabled();
                    let label = if crate::modules::window::is_hover_hide_auto_disabled() {
                        "Disable hide on hover (Auto)"
                    } else {
                        "Disable hide on hover"
                    };
                    let _ = check_item.set_checked(disabled);
                    let _ = check_item.set_text(label);
                }
            }
        }
    }
}

pub fn update_color_menu_labels(app: &tauri::AppHandle) {
    if let Some(menu) = active_menu(app) {
        let normal_controls_enabled = normal_mode_only_controls_enabled();
        let normal_settings = load_settings_for_mode(app, WindowMode::Normal);

        if let Some(item) = menu.get("mini_window_mode") {
            if let Some(check_item) = item.as_check_menuitem() {
                let _ = check_item.set_checked(mode::current_mode() == WindowMode::Window);
                let _ = check_item.set_text("Mini-window mode");
            }
        }

        if let Some(item) = menu.get("local_ipv4") {
            if let Some(menu_item) = item.as_menuitem() {
                let _ = menu_item.set_text(local_ipv4_menu_text());
            }
        }

        // Update monitor checked states
        if let Ok(monitors) = app.available_monitors() {
            let selected = SELECTED_MONITOR_INDEX.load(Ordering::SeqCst);
            for (idx, _mon) in monitors.iter().enumerate() {
                let id = format!("monitor_{}", idx);
                if let Some(item) = menu.get(&id) {
                    if let Some(check_item) = item.as_check_menuitem() {
                        let _ = check_item.set_checked(idx == selected);
                        let _ = check_item.set_text(format!("Monitor {}", idx + 1));
                    }
                }
            }
        }

        // Update fancy animation disabled checked state
        if let Some(item) = menu.get("disable_animation") {
            if let Some(check_item) = item.as_check_menuitem() {
                let fancy_animation_disabled = WORD_BOUNCE_DISABLED.load(Ordering::SeqCst);
                let _ = check_item.set_checked(!fancy_animation_disabled);
                let _ = check_item.set_text("Fancy Animations");
            }
        }

        if let Some(item) = menu.get("blur_enabled") {
            if let Some(check_item) = item.as_check_menuitem() {
                let blur_enabled = BLUR_ENABLED.load(Ordering::SeqCst);
                let _ = check_item.set_checked(blur_enabled);
                let _ = check_item.set_text("Blur");
            }
        }

        // Update pause lyrics checked state
        if let Some(item) = menu.get("pause_lyrics") {
            if let Some(check_item) = item.as_check_menuitem() {
                let lyrics_paused = if normal_controls_enabled {
                    LYRICS_PAUSED.load(Ordering::SeqCst)
                } else {
                    lyrics_paused_for_mode(WindowMode::Normal)
                };
                let _ = check_item.set_checked(lyrics_paused);
                let _ = check_item.set_text("Pause Lyrics");
                let _ = check_item.set_enabled(normal_controls_enabled);
            }
        }

        if let Some(item) = menu.get("always_on_top") {
            if let Some(check_item) = item.as_check_menuitem() {
                let enabled = if normal_controls_enabled {
                    ALWAYS_ON_TOP_ENABLED.load(Ordering::SeqCst)
                } else {
                    normal_settings.always_on_top_enabled
                };
                let _ = check_item.set_checked(enabled);
                let _ = check_item.set_text("Always On Top");
                let _ = check_item.set_enabled(normal_controls_enabled);
            }
        }

        if let Some(item) = menu.get("disable_hover_hide") {
            if let Some(check_item) = item.as_check_menuitem() {
                let (enabled, label) = if normal_controls_enabled {
                    let enabled = crate::modules::window::is_hover_hide_effectively_disabled();
                    let label = if crate::modules::window::is_hover_hide_auto_disabled() {
                        "Disable hide on hover (Auto)"
                    } else {
                        "Disable hide on hover"
                    };
                    (enabled, label)
                } else {
                    (normal_settings.disable_hover_hide, "Disable hide on hover")
                };
                let _ = check_item.set_checked(enabled);
                let _ = check_item.set_text(label);
                let _ = check_item.set_enabled(normal_controls_enabled);
            }
        }
    }

    sync_server_update_menu(app);
}

// Build menu items (common to both versions)
pub fn build_menu_items(
    app: &tauri::AppHandle,
) -> Result<Vec<MenuItemKind<tauri::Wry>>, Box<dyn std::error::Error>> {
    let mut menu_items: Vec<MenuItemKind<_>> = Vec::new();

    let local_ipv4_item = MenuItem::with_id(
        app,
        "local_ipv4",
        local_ipv4_menu_text(),
        true,
        None::<&str>,
    )?;
    menu_items.push(local_ipv4_item.kind());

    let sep = PredefinedMenuItem::separator(app)?;
    menu_items.push(sep.kind());

    let mode_toggle = CheckMenuItem::with_id(
        app,
        "mini_window_mode",
        "Mini-window mode",
        true,
        mode::current_mode() == WindowMode::Window,
        None::<&str>,
    )?;
    menu_items.push(mode_toggle.kind());

    let sep = PredefinedMenuItem::separator(app)?;
    menu_items.push(sep.kind());

    if let Ok(monitors) = app.available_monitors() {
        let selected = SELECTED_MONITOR_INDEX.load(Ordering::SeqCst);
        for (idx, _monitor) in monitors.iter().enumerate() {
            let id = format!("monitor_{}", idx);
            let name = format!("Monitor {}", idx + 1);
            menu_items.push(
                CheckMenuItem::with_id(app, &id, name, true, selected == idx, None::<&str>)?.kind(),
            );
        }
    }

    let sep = PredefinedMenuItem::separator(app)?;
    menu_items.push(sep.kind());

    let fancy_animation_disabled = WORD_BOUNCE_DISABLED.load(Ordering::SeqCst);
    let disable_animation_item = CheckMenuItem::with_id(
        app,
        "disable_animation",
        "Fancy Animations",
        true,
        !fancy_animation_disabled,
        None::<&str>,
    )?;
    let blur_enabled = BLUR_ENABLED.load(Ordering::SeqCst);
    let blur_item = CheckMenuItem::with_id(
        app,
        "blur_enabled",
        "Blur",
        true,
        blur_enabled,
        None::<&str>,
    )?;
    let disable_hover_hide = DISABLE_HOVER_HIDE.load(Ordering::SeqCst);
    let disable_hover_hide_item = CheckMenuItem::with_id(
        app,
        "disable_hover_hide",
        "Disable hide on hover",
        true,
        disable_hover_hide,
        None::<&str>,
    )?;
    let always_on_top_enabled = ALWAYS_ON_TOP_ENABLED.load(Ordering::SeqCst);
    let always_on_top_item = CheckMenuItem::with_id(
        app,
        "always_on_top",
        "Always On Top",
        true,
        always_on_top_enabled,
        None::<&str>,
    )?;
    menu_items.push(always_on_top_item.kind());
    menu_items.push(disable_hover_hide_item.kind());

    let sep = PredefinedMenuItem::separator(app)?;
    menu_items.push(sep.kind());
    menu_items.push(disable_animation_item.kind());
    menu_items.push(blur_item.kind());

    let sep = PredefinedMenuItem::separator(app)?;
    menu_items.push(sep.kind());

    let lyrics_paused = LYRICS_PAUSED.load(Ordering::SeqCst);
    let pause_lyrics_item = CheckMenuItem::with_id(
        app,
        "pause_lyrics",
        "Pause Lyrics",
        true,
        lyrics_paused,
        None::<&str>,
    )?;
    menu_items.push(pause_lyrics_item.kind());

    let open_guide_item = MenuItem::with_id(app, "open_guide", "Open Guide", true, None::<&str>)?;
    menu_items.push(open_guide_item.kind());

    let restart_item = MenuItem::with_id(app, "restart", "Restart App", true, None::<&str>)?;
    menu_items.push(restart_item.kind());

    let sep = PredefinedMenuItem::separator(app)?;
    menu_items.push(sep.kind());

    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    menu_items.push(quit.kind());
    let sep = PredefinedMenuItem::separator(app)?;
    menu_items.push(sep.kind());
    let footer_credit = MenuItem::with_id(
        app,
        "footer_credit",
        "Made with ♥ by Necra",
        false,
        None::<&str>,
    )?;
    menu_items.push(footer_credit.kind());

    Ok(menu_items)
}

// Handle menu events (common logic)
pub fn handle_menu_event(app: &tauri::AppHandle, event_id: &str) {
    if event_id == "quit" {
        app.exit(0);
        return;
    }

    if event_id == "server_update_action" {
        crate::modules::update::handle_tray_action(app.clone());
        return;
    }

    if event_id == "restart" {
        crate::app_runtime::restart_app(app);
        return;
    }

    if event_id == "open_guide" {
        crate::app_runtime::open_welcome_in_main_window(app);
        return;
    }

    if event_id == "local_ipv4" {
        let _ = app.opener().open_url(local_ipv4_url(), None::<&str>);
        return;
    }

    if event_id == "mini_window_mode" {
        let target_mode = if mode::current_mode() == WindowMode::Window {
            WindowMode::Normal
        } else {
            WindowMode::Window
        };
        crate::app_runtime::switch_window_mode(app, target_mode);
        return;
    }

    if event_id == "disable_animation" {
        scripts::toggle_fancy_animation_disabled(app.clone());
        return;
    }

    if event_id == "blur_enabled" {
        scripts::toggle_blur_enabled(app.clone());
        return;
    }

    if event_id == "pause_lyrics" {
        if !normal_mode_only_controls_enabled() {
            update_color_menu_labels(app);
            return;
        }
        toggle_lyrics_pause(app);
        return;
    }

    if event_id == "always_on_top" {
        if !normal_mode_only_controls_enabled() {
            update_color_menu_labels(app);
            return;
        }
        let new_state = !ALWAYS_ON_TOP_ENABLED.load(Ordering::SeqCst);
        ALWAYS_ON_TOP_ENABLED.store(new_state, Ordering::SeqCst);

        if let Some(window) = mode::active_window(app) {
            apply_always_on_top_preference(&window);
        }

        save_current_settings(app);
        update_color_menu_labels(app);
        return;
    }

    if event_id == "disable_hover_hide" {
        if !normal_mode_only_controls_enabled() {
            update_color_menu_labels(app);
            return;
        }
        let new_state = !DISABLE_HOVER_HIDE.load(Ordering::SeqCst);
        DISABLE_HOVER_HIDE.store(new_state, Ordering::SeqCst);

        if new_state {
            if let Some(window) = mode::active_window(app) {
                force_show_immediate(&window);
            }
        }

        save_current_settings(app);
        update_color_menu_labels(app);
        return;
    }

    if mode::current_mode() == WindowMode::Window && event_id.starts_with("monitor_") {
        return;
    }

    if event_id == "actions_label" {
        return;
    }

    if let Some(monitor_idx) = event_id.strip_prefix("monitor_") {
        if let Ok(idx) = monitor_idx.parse::<usize>() {
            SELECTED_MONITOR_INDEX.store(idx, Ordering::SeqCst);

            if let Some(window) = mode::get_window(app, WindowMode::Normal) {
                setup_window_position(app, &window);
            }
            save_current_settings(app);
            update_color_menu_labels(app);
        }
    }
}

// Toggle lyrics pause state
pub fn toggle_lyrics_pause(app: &tauri::AppHandle) {
    if !normal_mode_only_controls_enabled() {
        update_color_menu_labels(app);
        return;
    }

    let current_state = LYRICS_PAUSED.load(Ordering::SeqCst);
    let new_state = !current_state;

    LYRICS_PAUSED.store(new_state, Ordering::SeqCst);
    set_lyrics_paused_for_mode(WindowMode::Normal, new_state);

    if let Some(window) = mode::get_window(app, WindowMode::Normal) {
        scripts::apply_lyrics_paused(&window, new_state);
        apply_always_on_top_preference(&window);
    }

    // Update menu item label
    if let Some(menu) = active_menu(app) {
        if let Some(item) = menu.get("pause_lyrics") {
            if let Some(check_item) = item.as_check_menuitem() {
                let _ = check_item.set_checked(new_state);
                let _ = check_item.set_text("Pause Lyrics");
            }
        }
    }

    println!("Lyrics paused: {}", new_state);
}

pub fn sync_server_update_menu(app: &tauri::AppHandle) {
    let Some(menu) = active_menu(app) else { return };
    let descriptor = crate::modules::update::tray_menu_descriptor();

    match descriptor {
        Some((text, enabled)) => {
            let item = if let Ok(mut slot) = SERVER_UPDATE_MENU_ITEM.lock() {
                if let Some(item) = slot.as_ref() {
                    item.clone()
                } else {
                    let Ok(created) = MenuItem::with_id(
                        app,
                        "server_update_action",
                        &text,
                        enabled,
                        None::<&str>,
                    ) else {
                        return;
                    };
                    *slot = Some(created.clone());
                    created
                }
            } else {
                return;
            };

            let separator = if let Ok(mut slot) = SERVER_UPDATE_SEPARATOR.lock() {
                if let Some(separator) = slot.as_ref() {
                    separator.clone()
                } else {
                    let Ok(created) = PredefinedMenuItem::separator(app) else {
                        return;
                    };
                    *slot = Some(created.clone());
                    created
                }
            } else {
                return;
            };

            let _ = item.set_text(&text);
            let _ = item.set_enabled(enabled);

            if menu.get("server_update_action").is_none() {
                let _ = menu.insert(&item, 0);
            }

            if menu.get(separator.id()).is_none() {
                let _ = menu.insert(&separator, 1);
            }
        }
        None => {
            if menu.get("server_update_action").is_some() {
                if let Ok(slot) = SERVER_UPDATE_MENU_ITEM.lock() {
                    if let Some(item) = slot.as_ref() {
                        let _ = menu.remove(item);
                    }
                }
            }
            if let Ok(slot) = SERVER_UPDATE_SEPARATOR.lock() {
                if let Some(separator) = slot.as_ref() {
                    let _ = menu.remove(separator);
                }
            }
        }
    }
}
