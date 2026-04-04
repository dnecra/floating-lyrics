use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// Returns true if a GET request to `http://ip:port/path` returns HTTP 200.
pub fn is_endpoint_reachable(ip: &str, port: u16, path: &str) -> bool {
    let addr: SocketAddr = match format!("{ip}:{port}").parse() {
        Ok(a) => a,
        Err(_) => return false,
    };

    if let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
        let req = format!("GET {path} HTTP/1.0\r\nHost: {ip}\r\nConnection: close\r\n\r\n");
        if stream.write_all(req.as_bytes()).is_err() {
            return false;
        }
        let mut buf = [0u8; 512];
        if let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                return false;
            }
            let s = String::from_utf8_lossy(&buf[..n]);
            return s.contains("HTTP/1.1 200") || s.contains("HTTP/1.0 200") || s.contains(" 200 ");
        }
    }
    false
}

/// Tries the primary IP first, then the optional fallback.
/// Returns the working URL or None if neither is reachable.
pub fn get_working_url(
    primary_ip: &str,
    fallback_ip: Option<&str>,
    port: u16,
    path: &str,
) -> Option<String> {
    if is_endpoint_reachable(primary_ip, port, path) {
        return Some(format!("http://{primary_ip}:{port}{path}"));
    }
    if let Some(fb) = fallback_ip {
        if is_endpoint_reachable(fb, port, path) {
            return Some(format!("http://{fb}:{port}{path}"));
        }
    }
    None
}

/// Background thread that monitors connectivity to the lyrics endpoint.
/// - When the server comes online -> navigate the window to lyrics.
/// - When the server goes offline -> hide the window.
/// - Connectivity changes (IP switches) -> re-navigate.
///
/// Note: this monitor watches the *lyrics* path only. The welcome page is
/// a one-shot navigation handled at startup / from the tray menu.
pub fn start_connectivity_monitor(
    app_handle: tauri::AppHandle,
    primary_ip: &'static str,
    fallback_ip: Option<&'static str>,
    port: u16,
    lyrics_path: &'static str,
    scripts: &'static crate::modules::scripts::Scripts,
    initial_lyrics_url: Option<String>,
) {
    use crate::modules::{mode, settings::*, window};

    std::thread::spawn(move || {
        let mut last_lyrics_url: Option<String> = initial_lyrics_url;
        let mut was_reachable = last_lyrics_url.is_some();

        loop {
            std::thread::sleep(Duration::from_secs(3));

            // While the window is on the welcome page, don't navigate away.
            if mode::current_mode() == mode::WindowMode::Normal
                && window::WELCOME_MODE_ACTIVE.load(std::sync::atomic::Ordering::SeqCst)
            {
                continue;
            }

            let Some(win) = mode::active_window(&app_handle) else {
                continue;
            };
            let current_mode = mode::current_mode();

            let current_url = get_working_url(primary_ip, fallback_ip, port, lyrics_path);

            if let Some(ref url) = current_url {
                let url_changed = last_lyrics_url.as_ref() != Some(url);
                if url_changed {
                    // Server came online or IP switched -- navigate to new URL.
                    let _ = win.navigate(url.parse().expect("valid URL"));
                    apply_connected_window_state(&win, scripts, current_mode);
                    if current_mode == mode::WindowMode::Window {
                        let _ = win.show();
                    } else {
                        window::show_without_focus(&win);
                    }
                    window::enforce_topmost(&win);
                    was_reachable = true;
                } else if !was_reachable {
                    // Same URL but we previously marked it as down -- show again.
                    if current_mode == mode::WindowMode::Window {
                        let _ = win.show();
                    } else {
                        window::show_without_focus(&win);
                    }
                    window::enforce_topmost(&win);
                    was_reachable = true;
                }

                crate::modules::scripts::apply_blur_enabled(
                    &win,
                    BLUR_ENABLED.load(std::sync::atomic::Ordering::SeqCst),
                );
            } else if was_reachable {
                // Server went offline.
                let _ = win.hide();
                was_reachable = false;
            }

            last_lyrics_url = current_url;
        }
    });
}

fn apply_connected_window_state(
    win: &tauri::WebviewWindow,
    scripts: &'static crate::modules::scripts::Scripts,
    mode: crate::modules::mode::WindowMode,
) {
    use crate::modules::settings::*;
    use std::sync::atomic::Ordering;

    match mode {
        crate::modules::mode::WindowMode::Normal => {
            let _ = win.eval(scripts.transparent_bg_script);
            let _ = win.eval(scripts.layout_hover_script);
            let _ = win.eval(scripts.close_window_script);
            let _ = win.eval(
                "if (window.__pushLayoutHoverBounds) { try { window.__pushLayoutHoverBounds(); } catch (_) {} }",
            );
        }
        crate::modules::mode::WindowMode::Window => {}
    }

    crate::modules::scripts::apply_blur_enabled(win, BLUR_ENABLED.load(Ordering::SeqCst));

    if WORD_BOUNCE_DISABLED.load(Ordering::SeqCst) {
        crate::modules::scripts::apply_fancy_animation_disabled(win);
    }
}
