use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::tray::TrayIconBuilder;
use tauri::webview::Color;
use tauri::{menu::Menu, Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg(target_os = "windows")]
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU, VK_SHIFT};

use crate::modules::{
    click_through, commands, lock, menu,
    mode::{self, WindowMode},
    network, scripts, settings, update, window,
};

// â”€â”€ Serverless remote endpoints â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
const SERVERLESS_PRIMARY_IP: &str = "192.168.99.47";
const SERVERLESS_FALLBACK_IP: &str = "192.168.0.101";
const SERVERLESS_PORT: u16 = 80;
const SERVERLESS_LYRICS_PATH: &str = "/lyrics";

// â”€â”€ Embedded-server endpoints (standalone) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
const LOCAL_HOST: &str = "127.0.0.1";
const LOCAL_PORT: u16 = 1312;
const LOCAL_LYRICS_PATH: &str = "/lyrics";
const LOCAL_WELCOME_PATH: &str = "/welcome";

// â”€â”€ Embedded executable paths (relative to resource dir) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
const STANDALONE_EXE_RELATIVE: &str = "source/lyrics-smtc-x64.exe";

// â”€â”€ Scripts bundled at compile time â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
const TRANSPARENT_BG_SCRIPT: &str = include_str!("../scripts/transparent-bg.js");
const LAYOUT_HOVER_SCRIPT: &str = include_str!("../scripts/layout-hover-bounds.js");
const CLOSE_WINDOW_SCRIPT: &str = include_str!("../scripts/close-window-control.js");
const WINDOW_MODE_INIT_SCRIPT: &str = r#"
    (() => {
        const key = 'lyricsSettings';
        const current = JSON.parse(localStorage.getItem(key) || '{}');
        current.lyricsDisplayMode = 'fixed-2';
        localStorage.setItem(key, JSON.stringify(current));
    })();
"#;

static SCRIPTS: scripts::Scripts = scripts::Scripts {
    transparent_bg_script: TRANSPARENT_BG_SCRIPT,
    layout_hover_script: LAYOUT_HOVER_SCRIPT,
    close_window_script: CLOSE_WINDOW_SCRIPT,
};

const STARTUP_INJECTION_PASSES: u32 = 8;
const SERVER_READY_TIMEOUT_SECS: u64 = 30;
const SERVER_READY_POLL_MS: u64 = 250;

// â”€â”€ Embedded child process handle â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
lazy_static::lazy_static! {
    static ref EMBEDDED_SERVER_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
    #[cfg(target_os = "windows")]
    static ref SERVER_JOB: Mutex<Option<isize>> = Mutex::new(None);
    static ref RUNTIME_VARIANT: Mutex<Option<Variant>> = Mutex::new(None);
    static ref RUNTIME_ENDPOINT: Mutex<Option<RuntimeEndpoint>> = Mutex::new(None);
    static ref RUNTIME_EMBEDDED_EXE: Mutex<Option<&'static str>> = Mutex::new(None);
}

static APP_EXITING: AtomicBool = AtomicBool::new(false);
static STARTUP_COMPLETE: AtomicBool = AtomicBool::new(false);
static STARTUP_SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);

// â”€â”€ App variant â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Serverless,
    Standalone,
}

// â”€â”€ Per-variant runtime config â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
#[derive(Clone, Copy)]
struct RuntimeConfig {
    /// IP / hostname of the lyrics server.
    primary_ip: &'static str,
    fallback_ip: Option<&'static str>,
    port: u16,
    lyrics_path: &'static str,
    /// Whether this variant ships an embedded server exe.
    embedded_exe: Option<&'static str>,
}

#[derive(Clone, Copy)]
struct RuntimeEndpoint {
    primary_ip: &'static str,
    fallback_ip: Option<&'static str>,
    port: u16,
    lyrics_path: &'static str,
}

impl RuntimeConfig {
    fn for_variant(variant: Variant) -> Self {
        match variant {
            Variant::Serverless => Self {
                primary_ip: SERVERLESS_PRIMARY_IP,
                fallback_ip: Some(SERVERLESS_FALLBACK_IP),
                port: SERVERLESS_PORT,
                lyrics_path: SERVERLESS_LYRICS_PATH,
                embedded_exe: None,
                // Serverless has no local server â†’ no /welcome endpoint.
            },
            Variant::Standalone => Self {
                primary_ip: LOCAL_HOST,
                fallback_ip: None,
                port: LOCAL_PORT,
                lyrics_path: LOCAL_LYRICS_PATH,
                embedded_exe: Some(STANDALONE_EXE_RELATIVE),
            },
        }
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Entry point
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub fn run(variant: Variant) {
    let cfg = RuntimeConfig::for_variant(variant);

    if !lock::acquire_app_lock() {
        eprintln!("Another instance of Floating Lyrics is already running. Exiting.");
        std::process::exit(0);
    }

    // Serverless feature: local API thread
    #[cfg(feature = "serverless")]
    if variant == Variant::Serverless {
        use std::sync::{atomic::AtomicBool, Arc};
        let alive = Arc::new(AtomicBool::new(true));
        let alive_clone = Arc::clone(&alive);
        thread::spawn(move || run_local_api(alive_clone));
    }

    #[cfg(target_os = "windows")]
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    #[cfg(target_os = "windows")]
    configure_webview2_hardware_acceleration();

    if let Ok(mut slot) = RUNTIME_ENDPOINT.lock() {
        *slot = Some(RuntimeEndpoint {
            primary_ip: cfg.primary_ip,
            fallback_ip: cfg.fallback_ip,
            port: cfg.port,
            lyrics_path: cfg.lyrics_path,
        });
    }
    if let Ok(mut slot) = RUNTIME_VARIANT.lock() {
        *slot = Some(variant);
    }
    if let Ok(mut slot) = RUNTIME_EMBEDDED_EXE.lock() {
        *slot = cfg.embedded_exe;
    }

    let run_result = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .on_page_load(|webview, payload| {
            if APP_EXITING.load(Ordering::SeqCst) {
                return;
            }
            let label = webview.label().to_string();
            if label != mode::NORMAL_WINDOW_LABEL && label != mode::WINDOW_MODE_LABEL {
                return;
            }
            let url = payload.url().to_string();

            if label == mode::NORMAL_WINDOW_LABEL {
                let _ = webview.eval(TRANSPARENT_BG_SCRIPT);
                // Seamlessly switch between welcome-mode and lyrics-mode based on URL.
                if is_welcome_url(&url) {
                    window::enter_welcome_mode(
                        &webview
                            .app_handle()
                            .get_webview_window(mode::NORMAL_WINDOW_LABEL)
                            .expect("main window"),
                    );
                } else {
                    let coming_from_welcome =
                        window::WELCOME_MODE_ACTIVE.load(std::sync::atomic::Ordering::SeqCst);
                    window::exit_welcome_mode(
                        &webview
                            .app_handle()
                            .get_webview_window(mode::NORMAL_WINDOW_LABEL)
                            .expect("main window"),
                    );
                    scripts::inject_scripts_rapidly(
                        webview
                            .app_handle()
                            .get_webview_window(mode::NORMAL_WINDOW_LABEL)
                            .expect("main window"),
                        &SCRIPTS,
                        STARTUP_INJECTION_PASSES,
                        WindowMode::Normal,
                    );
                    if coming_from_welcome {
                        let window = webview
                            .app_handle()
                            .get_webview_window(mode::NORMAL_WINDOW_LABEL)
                            .expect("main window");
                        let _ = window.hide();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(1000));
                            window::animate_show_and_focus(&window);
                        });
                    }
                }
            } else if !is_welcome_url(&url) {
                let window = webview
                    .app_handle()
                    .get_webview_window(mode::WINDOW_MODE_LABEL)
                    .expect("window mode window");
                scripts::inject_scripts_rapidly(
                    window.clone(),
                    &SCRIPTS,
                    STARTUP_INJECTION_PASSES,
                    WindowMode::Window,
                );
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::set_click_through,
            commands::set_blur_enabled,
            commands::update_layout_container_bounds,
            commands::toggle_window_mode_always_on_top,
            commands::minimize_window_mode,
            commands::toggle_window_mode_fullscreen,
            commands::close_window_mode,
            commands::start_window_mode_dragging,
            commands::log_hover_probe,
            commands::get_window_mode_chrome_state,
            commands::close_app,
        ])
        .setup(move |app| {
            // Build tray menu early so bootstrap download state is visible immediately.
            let menu_items = menu::build_menu_items(&app.handle())?;
            let menu_refs: Vec<&dyn tauri::menu::IsMenuItem<_>> = menu_items
                .iter()
                .map(|i| i as &dyn tauri::menu::IsMenuItem<_>)
                .collect();
            let tray_menu = Menu::with_items(app, menu_refs.as_slice())?;
            menu::set_runtime_tray_menu(tray_menu.clone());

            let _tray = TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .expect("default window icon")
                        .clone(),
                )
                .tooltip("Floating Lyrics")
                .menu(&tray_menu)
                .on_tray_icon_event(|_tray, _event| {})
                .on_menu_event(move |app, event| {
                    menu::handle_menu_event(app, event.id.as_ref());
                })
                .build(app)?;

            let app_handle = app.handle().clone();
            update::initialize(app_handle.clone(), variant, cfg.embedded_exe);

            thread::spawn(move || {
                if let Err(error) = prepare_runtime_server(&app_handle, variant, cfg) {
                    eprintln!("Failed to prepare standalone server: {error}");
                }

                let app_for_finish = app_handle.clone();
                let _ = app_handle.run_on_main_thread(move || {
                    if let Err(error) = complete_startup(&app_for_finish, cfg) {
                        eprintln!("Failed to finalize app startup: {error}");
                    }
                });
            });
            Ok(())
        })
        .run(tauri::generate_context!());

    stop_embedded_server();
    run_result.expect("error while running tauri application");
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Public helpers called from menu / other modules
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub fn open_welcome_in_main_window(app: &tauri::AppHandle) {
    STARTUP_SHOW_REQUESTED.store(true, Ordering::SeqCst);
    if !STARTUP_COMPLETE.load(Ordering::SeqCst) {
        return;
    }
    if mode::current_mode() != WindowMode::Normal {
        switch_window_mode(app, WindowMode::Normal);
    }
    let Some(window) = app.get_webview_window(mode::NORMAL_WINDOW_LABEL) else {
        return;
    };
    let url = current_runtime_endpoint()
        .and_then(|endpoint| {
            network::get_working_url(
                endpoint.primary_ip,
                endpoint.fallback_ip,
                endpoint.port,
                LOCAL_WELCOME_PATH,
            )
        })
        .unwrap_or_else(|| format!("http://{}:{}{}", LOCAL_HOST, LOCAL_PORT, LOCAL_WELCOME_PATH));
    window::enter_welcome_mode(&window);
    let _ = window.navigate(url.parse().expect("valid URL"));
    window::animate_show_and_focus(&window);
}

fn prepare_runtime_server(
    app: &tauri::AppHandle,
    variant: Variant,
    cfg: RuntimeConfig,
) -> Result<(), String> {
    if variant != Variant::Standalone {
        return Ok(());
    }

    update::ensure_server_ready(app, variant, cfg.embedded_exe)?;

    if let Some(exe) = cfg.embedded_exe {
        start_embedded_server(app.clone(), exe);
        if !wait_for_server_ready(
            cfg.primary_ip,
            cfg.fallback_ip,
            cfg.port,
            cfg.lyrics_path,
            Duration::from_secs(SERVER_READY_TIMEOUT_SECS),
        ) {
            return Err(format!(
                "Standalone server did not become ready at {}:{}{} within {} seconds",
                cfg.primary_ip, cfg.port, cfg.lyrics_path, SERVER_READY_TIMEOUT_SECS
            ));
        }
    }

    Ok(())
}

fn complete_startup(app: &tauri::AppHandle, cfg: RuntimeConfig) -> tauri::Result<()> {
    let window = ensure_main_window(app)?;

    mode::set_current_mode(WindowMode::Normal);
    let mut loaded_settings = settings::load_settings_for_mode(app, WindowMode::Normal);
    settings::set_lyrics_paused_for_mode(
        WindowMode::Normal,
        settings::lyrics_paused_for_mode(WindowMode::Normal),
    );
    settings::set_lyrics_paused_for_mode(
        WindowMode::Window,
        settings::lyrics_paused_for_mode(WindowMode::Window),
    );
    settings::LYRICS_PAUSED.store(
        settings::lyrics_paused_for_mode(WindowMode::Normal),
        Ordering::SeqCst,
    );

    loaded_settings.click_through_enabled = true;
    settings::apply_loaded_settings(&loaded_settings);

    window::apply_windows_visual_tweaks(&window);
    window::setup_window_position(app, &window);
    window::setup_window_events(&window);
    let _ = ensure_window_mode_window(app);

    let initial_url =
        network::get_working_url(cfg.primary_ip, cfg.fallback_ip, cfg.port, cfg.lyrics_path);
    let initial_lyrics_url = initial_url.clone();
    if let Some(url) = initial_url.as_ref() {
        let _ = window.navigate(url.parse().expect("valid URL"));
        let _ = window.eval(TRANSPARENT_BG_SCRIPT);
        scripts::inject_scripts_rapidly(
            window.clone(),
            &SCRIPTS,
            STARTUP_INJECTION_PASSES,
            WindowMode::Normal,
        );
    }

    window::apply_settings(app, &loaded_settings, &SCRIPTS);
    window::exit_welcome_mode(&window);

    network::start_connectivity_monitor(
        app.clone(),
        cfg.primary_ip,
        cfg.fallback_ip,
        cfg.port,
        cfg.lyrics_path,
        &SCRIPTS,
        initial_lyrics_url,
    );

    window::start_monitor_watcher(window.clone());
    window::start_layout_hover_controller(window.clone());
    start_click_through_hotkey_guard(app.clone());

    menu::update_color_menu_labels(app);
    STARTUP_COMPLETE.store(true, Ordering::SeqCst);
    show_main_window(app);
    window::apply_always_on_top_preference(&window);

    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    if mode::current_mode() != WindowMode::Normal {
        switch_window_mode(app, WindowMode::Normal);
        return;
    }

    let Some(window) = app.get_webview_window(mode::NORMAL_WINDOW_LABEL) else {
        return;
    };

    if STARTUP_SHOW_REQUESTED.swap(false, Ordering::SeqCst)
        || !window.is_visible().unwrap_or(false)
    {
        window::show_and_focus_immediate(&window);
        window::apply_always_on_top_preference(&window);
    }
}
pub fn restart_app(app: &tauri::AppHandle) {
    APP_EXITING.store(true, Ordering::SeqCst);
    stop_embedded_server();
    lock::release_app_lock();
    app.restart();
}

pub fn mark_app_exiting() {
    APP_EXITING.store(true, Ordering::SeqCst);
}

pub fn stop_embedded_server_process() {
    stop_embedded_server();
}

pub fn start_embedded_server_process(app: &tauri::AppHandle) -> Result<(), String> {
    let exe_relative = RUNTIME_EMBEDDED_EXE
        .lock()
        .map_err(|_| "Embedded server state is unavailable".to_string())?
        .to_owned()
        .ok_or_else(|| "This runtime variant has no embedded server".to_string())?;
    start_embedded_server(app.clone(), exe_relative);
    Ok(())
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// URL helpers
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub fn is_welcome_url(url: &str) -> bool {
    url.contains("/welcome") || url.contains("/guide")
}

pub fn current_variant() -> Option<Variant> {
    RUNTIME_VARIANT.lock().ok().and_then(|slot| *slot)
}

fn current_runtime_endpoint() -> Option<RuntimeEndpoint> {
    RUNTIME_ENDPOINT.lock().ok().and_then(|slot| *slot)
}

fn current_lyrics_url() -> Option<String> {
    let endpoint = current_runtime_endpoint()?;
    network::get_working_url(
        endpoint.primary_ip,
        endpoint.fallback_ip,
        endpoint.port,
        endpoint.lyrics_path,
    )
}

fn wait_for_server_ready(
    primary_ip: &str,
    fallback_ip: Option<&str>,
    port: u16,
    path: &str,
    timeout: Duration,
) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if network::get_working_url(primary_ip, fallback_ip, port, path).is_some() {
            return true;
        }
        thread::sleep(Duration::from_millis(SERVER_READY_POLL_MS));
    }
    network::get_working_url(primary_ip, fallback_ip, port, path).is_some()
}

fn webview_data_directory(app: &tauri::AppHandle, folder_name: &str) -> std::path::PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("Failed to get app data directory")
        .join(folder_name);
    std::fs::create_dir_all(&data_dir).expect("Failed to create webview data directory");
    data_dir
}

fn ensure_main_window(app: &tauri::AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    if let Some(window) = app.get_webview_window(mode::NORMAL_WINDOW_LABEL) {
        return Ok(window);
    }

    let window_config = app
        .config()
        .app
        .windows
        .first()
        .expect("main window config must exist");

    tauri::WebviewWindowBuilder::from_config(app, window_config)?
        .data_directory(webview_data_directory(app, "main-webview"))
        .build()
}

fn ensure_window_mode_window(app: &tauri::AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    if let Some(window) = app.get_webview_window(mode::WINDOW_MODE_LABEL) {
        return Ok(window);
    }

    let window_settings = settings::load_settings_for_mode(app, WindowMode::Window);
    let width = window_settings.window_mode_width.unwrap_or(360).max(256);
    let height = window_settings.window_mode_height.unwrap_or(360).max(256);
    let data_dir = webview_data_directory(app, "window-mode-webview");

    let builder = WebviewWindowBuilder::new(
        app,
        mode::WINDOW_MODE_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("Floating Lyrics")
    .decorations(false)
    .transparent(false)
    .background_color(Color(12, 16, 24, 255))
    .resizable(true)
    .maximizable(true)
    .minimizable(true)
    .skip_taskbar(false)
    .visible(false)
    .inner_size(width as f64, height as f64)
    .min_inner_size(256.0, 256.0)
    .initialization_script(WINDOW_MODE_INIT_SCRIPT)
    .data_directory(data_dir);

    let builder = if let (Some(x), Some(y)) =
        (window_settings.window_mode_x, window_settings.window_mode_y)
    {
        builder.position(x as f64, y as f64)
    } else {
        builder.center()
    };

    let window = builder.build()?;
    let _ = window.set_background_color(Some(Color(12, 16, 24, 255)));

    window::setup_window_events(&window);
    window::setup_window_mode_state_tracking(app.clone(), &window);
    Ok(window)
}

pub fn switch_window_mode(app: &tauri::AppHandle, target_mode: WindowMode) {
    let current_mode = mode::current_mode();
    if current_mode == target_mode {
        if let Some(window) = mode::active_window(app) {
            window::show_and_focus_immediate(&window);
            window::apply_always_on_top_preference(&window);
        }
        menu::update_color_menu_labels(app);
        return;
    }

    settings::set_lyrics_paused_for_mode(
        current_mode,
        settings::LYRICS_PAUSED.load(Ordering::SeqCst),
    );
    settings::save_current_settings(app);

    let target_window = match target_mode {
        WindowMode::Normal => app.get_webview_window(mode::NORMAL_WINDOW_LABEL),
        WindowMode::Window => ensure_window_mode_window(app).ok(),
    };
    let Some(target_window) = target_window else {
        return;
    };

    if let Some(current_window) = mode::get_window(app, current_mode) {
        if current_mode == WindowMode::Window {
            window::animate_hide(&current_window);
        } else {
            let _ = current_window.hide();
        }
    }

    let loaded_settings = settings::load_settings_for_mode(app, target_mode);
    mode::set_current_mode(target_mode);
    settings::apply_loaded_settings(&loaded_settings);
    settings::LYRICS_PAUSED.store(
        settings::lyrics_paused_for_mode(target_mode),
        Ordering::SeqCst,
    );
    window::apply_settings_to_window(app, &target_window, &loaded_settings, &SCRIPTS, target_mode);
    scripts::apply_lyrics_paused(
        &target_window,
        settings::LYRICS_PAUSED.load(Ordering::SeqCst),
    );

    if let Some(url) = current_lyrics_url() {
        let needs_navigation = target_window
            .url()
            .map(|current| current.as_str() != url)
            .unwrap_or(true);

        if needs_navigation {
            let _ = target_window.navigate(url.parse().expect("valid URL"));
        }

        scripts::inject_scripts_rapidly(
            target_window.clone(),
            &SCRIPTS,
            if needs_navigation {
                STARTUP_INJECTION_PASSES
            } else {
                2
            },
            target_mode,
        );
    }

    match target_mode {
        WindowMode::Normal => {
            window::exit_welcome_mode(&target_window);
            window::animate_show_and_focus(&target_window);
        }
        WindowMode::Window => {
            let _ = target_window.set_ignore_cursor_events(false);
            let _ = target_window.set_focusable(true);
            window::animate_show_and_focus(&target_window);
            let _ = target_window.eval(
                "window.focus(); try { document.body && document.body.focus({ preventScroll: true }); } catch (_) {}",
            );
        }
    }

    window::apply_always_on_top_preference(&target_window);
    menu::update_color_menu_labels(app);
}

pub fn close_window_mode(app: &tauri::AppHandle) {
    if mode::current_mode() != WindowMode::Window {
        return;
    }

    if let Some(window) = app.get_webview_window(mode::WINDOW_MODE_LABEL) {
        let _ = window.close();
    }

    switch_window_mode(app, WindowMode::Normal);
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Click-through hotkey guard  (Alt+Shift+F  â†’  temporarily disable)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
fn start_click_through_hotkey_guard(app: tauri::AppHandle) {
    #[cfg(target_os = "windows")]
    thread::spawn(move || {
        let mut hotkey_active = false;
        let mut last_combo_down = false;
        loop {
            // Hotkey is suppressed while on welcome page or while paused.
            let runtime_active = mode::current_mode() == WindowMode::Normal
                && !settings::LYRICS_PAUSED.load(Ordering::SeqCst)
                && !window::WELCOME_MODE_ACTIVE.load(Ordering::SeqCst)
                && app
                    .get_webview_window(mode::NORMAL_WINDOW_LABEL)
                    .map(|w| w.is_visible().unwrap_or(true))
                    .unwrap_or(false);

            let combo_down = runtime_active && is_alt_shift_f_down();

            if combo_down && !last_combo_down {
                click_through::set_click_through_runtime_no_persist(&app, false);
                hotkey_active = true;
            } else if (!combo_down || !runtime_active) && hotkey_active {
                click_through::set_click_through_runtime_no_persist(&app, true);
                hotkey_active = false;
            }

            last_combo_down = combo_down;
            thread::sleep(Duration::from_millis(16));
        }
    });

    #[cfg(not(target_os = "windows"))]
    thread::spawn(move || loop {
        let _ = &app;
        thread::sleep(Duration::from_secs(60));
    });
}

#[cfg(target_os = "windows")]
fn is_alt_shift_f_down() -> bool {
    let alt = unsafe { GetAsyncKeyState(VK_MENU.0 as i32) } < 0;
    let shift = unsafe { GetAsyncKeyState(VK_SHIFT.0 as i32) } < 0;
    let f = unsafe { GetAsyncKeyState('F' as i32) } < 0;
    alt && shift && f
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Embedded server lifecycle
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
fn start_embedded_server(app: tauri::AppHandle, exe_relative: &str) {
    if network::is_endpoint_reachable(LOCAL_HOST, LOCAL_PORT, LOCAL_LYRICS_PATH) {
        return; // Already running (e.g. dev mode).
    }

    let Some(candidate) = update::managed_server_exe_path(&app, exe_relative) else {
        eprintln!("Could not resolve managed embedded server path: {exe_relative}");
        return;
    };
    if !candidate.exists() {
        eprintln!(
            "Managed embedded server executable is missing: {}",
            candidate.display()
        );
        return;
    }

    let mut cmd = std::process::Command::new(&candidate);
    cmd.current_dir(
        candidate
            .parent()
            .unwrap_or_else(|| std::path::Path::new(".")),
    );

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id();
            #[cfg(target_os = "windows")]
            attach_child_to_job_object(&child);
            if let Ok(mut slot) = EMBEDDED_SERVER_CHILD.lock() {
                *slot = Some(child);
            }
            println!(
                "Started embedded server: {} (pid {})",
                candidate.display(),
                pid
            );
        }
        Err(e) => {
            eprintln!(
                "Found server exe but failed to start {}: {e}",
                candidate.display()
            );
        }
    }
}

fn stop_embedded_server() {
    let mut child_opt = match EMBEDDED_SERVER_CHILD.lock() {
        Ok(mut g) => g.take(),
        Err(_) => None,
    };
    let Some(mut child) = child_opt.take() else {
        return;
    };

    #[cfg(target_os = "windows")]
    {
        let pid = child.id();
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "windows")]
fn attach_child_to_job_object(child: &std::process::Child) {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    let mut guard = match SERVER_JOB.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    if guard.is_none() {
        let Ok(job) = (unsafe { CreateJobObjectW(None, None) }) else {
            return;
        };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok.is_err() {
            return;
        }
        *guard = Some(job.0 as isize);
    }

    let Some(job_raw) = *guard else { return };
    let job = HANDLE(job_raw as *mut core::ffi::c_void);
    let process = HANDLE(child.as_raw_handle() as *mut core::ffi::c_void);
    let _ = unsafe { AssignProcessToJobObject(job, process) };
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// WebView2 hardware acceleration tweak (Windows only)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
#[cfg(target_os = "windows")]
fn configure_webview2_hardware_acceleration() {
    let current = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
    let mut args: Vec<String> = current
        .split_whitespace()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    args.retain(|a| {
        !matches!(
            a.as_str(),
            "--disable-gpu"
                | "--disable-gpu-compositing"
                | "--in-process-gpu"
                | "--disable-accelerated-2d-canvas"
                | "--disable-accelerated-video-decode"
        )
    });
    for wanted in [
        "--ignore-gpu-blocklist",
        "--enable-gpu-rasterization",
        "--enable-zero-copy",
    ] {
        if !args.iter().any(|a| a == wanted) {
            args.push(wanted.to_string());
        }
    }
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", args.join(" "));
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Serverless local API  (feature-gated)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
#[cfg(feature = "serverless")]
fn run_local_api(alive: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::Ordering;
    use tiny_http::{Header, Method, Response, Server};

    let server = match Server::http("127.0.0.1:32145") {
        Ok(s) => s,
        Err(_) => return,
    };

    let cors = Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap();
    let cors_methods =
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap();
    let cors_headers = Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap();
    let json_header = Header::from_bytes("Content-Type", "application/json").unwrap();

    while alive.load(Ordering::Relaxed) {
        let req = match server.recv_timeout(Duration::from_millis(200)) {
            Ok(Some(r)) => r,
            _ => continue,
        };

        if req.method() == &Method::Options {
            let mut resp = Response::empty(204);
            resp.add_header(cors.clone());
            resp.add_header(cors_methods.clone());
            resp.add_header(cors_headers.clone());
            let _ = req.respond(resp);
            continue;
        }

        match (req.method(), req.url()) {
            (&Method::Get, "/floating-lyrics/status") => {
                let mut resp = Response::from_string(r#"{"running":true}"#);
                resp.add_header(cors.clone());
                resp.add_header(json_header.clone());
                let _ = req.respond(resp);
            }
            (&Method::Post, "/floating-lyrics/toggle") => {
                let mut resp = Response::from_string(r#"{"ok":true}"#);
                resp.add_header(cors.clone());
                resp.add_header(json_header.clone());
                let _ = req.respond(resp);
                stop_embedded_server();
                std::process::exit(0);
            }
            _ => {
                let mut resp = Response::from_string("not found").with_status_code(404);
                resp.add_header(cors.clone());
                let _ = req.respond(resp);
            }
        }
    }
}
