use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::app_runtime::Variant;

const UPDATE_CHECK_DELAY_SECS: u64 = 10;
const UPDATE_CHECK_INTERVAL_SECS: u64 = 6 * 60 * 60;
const SERVER_UPDATE_FILE: &str = "server-update.json";
const GITHUB_API_BASE: &str = "https://api.github.com";
const STANDALONE_RELEASE_OWNER: &str = match option_env!("FLOATING_LYRICS_STANDALONE_RELEASE_OWNER")
{
    Some(value) => value,
    None => "dnecra",
};
const STANDALONE_RELEASE_REPO: &str = match option_env!("FLOATING_LYRICS_STANDALONE_RELEASE_REPO") {
    Some(value) => value,
    None => "lyrics-server",
};
const USER_AGENT: &str = "floating-lyrics-updater";

lazy_static::lazy_static! {
    static ref UPDATE_CONTEXT: Mutex<Option<UpdateContext>> = Mutex::new(None);
    static ref UPDATE_STATE: Mutex<ServerUpdateState> = Mutex::new(ServerUpdateState::Idle);
    static ref PENDING_RELEASE: Mutex<Option<ResolvedServerRelease>> = Mutex::new(None);
}

#[derive(Clone)]
struct UpdateContext {
    owner: &'static str,
    repo: &'static str,
    exe_relative: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubReleaseResponse {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

#[derive(Debug, Clone)]
struct ResolvedServerRelease {
    version: String,
    url: String,
    sha256: String,
    asset_name: String,
    exe_name: String,
}

#[derive(Debug, Clone)]
pub enum ServerUpdateState {
    Idle,
    Checking,
    Available,
    Downloading,
    Installing,
    Failed,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PersistedServerUpdate {
    installed_version: String,
}

pub fn initialize(app: AppHandle, variant: Variant, exe_relative: Option<&'static str>) {
    let Some(context) = configure_context(variant, exe_relative) else {
        return;
    };

    if let Ok(mut slot) = UPDATE_CONTEXT.lock() {
        *slot = Some(context);
    }

    crate::modules::menu::sync_server_update_menu(&app);

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(UPDATE_CHECK_DELAY_SECS));
        start_update_check(app.clone());

        loop {
            thread::sleep(Duration::from_secs(UPDATE_CHECK_INTERVAL_SECS));
            start_update_check(app.clone());
        }
    });
}

pub fn ensure_server_ready(
    app: &AppHandle,
    variant: Variant,
    exe_relative: Option<&'static str>,
) -> Result<(), String> {
    let Some(context) = configure_context(variant, exe_relative) else {
        return Ok(());
    };

    if let Ok(mut slot) = UPDATE_CONTEXT.lock() {
        *slot = Some(context.clone());
    }

    let target_path = resolve_target_path(app, &context)?;
    if target_path.exists() {
        println!(
            "Standalone server already available at {}",
            target_path.display()
        );
        return Ok(());
    }

    if let Some(dev_path) = local_dev_server_path(&context) {
        println!(
            "Using local development server executable at {}",
            dev_path.display()
        );
        return Ok(());
    }

    println!(
        "No local standalone server found. Bootstrapping from GitHub releases for {}/{}...",
        context.owner, context.repo
    );
    set_state(app, ServerUpdateState::Downloading);
    let release = fetch_latest_release(&context)?;
    install_release(app, &context, &release, false)?;
    persist_installed_version(app, &release.version)?;
    println!(
        "Standalone server bootstrap complete. Installed version {}",
        release.version
    );
    set_state(app, ServerUpdateState::Idle);
    Ok(())
}

pub fn managed_server_exe_path(app: &AppHandle, exe_relative: &str) -> Option<PathBuf> {
    let exe_name = Path::new(exe_relative).file_name()?;
    Some(
        app.path()
            .app_data_dir()
            .ok()?
            .join("server-bin")
            .join(exe_name),
    )
}

pub fn current_update_state() -> ServerUpdateState {
    UPDATE_STATE
        .lock()
        .map(|state| state.clone())
        .unwrap_or(ServerUpdateState::Idle)
}

pub fn tray_menu_descriptor() -> Option<(String, bool)> {
    match current_update_state() {
        ServerUpdateState::Idle | ServerUpdateState::Checking => None,
        ServerUpdateState::Available => {
            Some(("New version available, update now!".to_string(), true))
        }
        ServerUpdateState::Downloading | ServerUpdateState::Installing => {
            Some(("Downloading server...".to_string(), false))
        }
        ServerUpdateState::Failed => Some(("Update failed - Retry".to_string(), true)),
    }
}

pub fn start_update_check(app: AppHandle) {
    if !begin_transition_to_checking() {
        return;
    }

    thread::spawn(move || {
        let outcome = run_update_check(&app);
        if let Err(error) = outcome {
            set_failed_state(&app, error);
        }
    });
}

pub fn start_server_update(app: AppHandle) {
    let release = match current_update_state() {
        ServerUpdateState::Available | ServerUpdateState::Failed => {
            PENDING_RELEASE.lock().ok().and_then(|slot| slot.clone())
        }
        _ => None,
    };

    let Some(release) = release else {
        return;
    };

    if !begin_transition_to_downloading(&app) {
        return;
    }

    thread::spawn(move || {
        let result =
            update_context().and_then(|context| install_release(&app, &context, &release, true));
        match result {
            Ok(()) => {
                if let Err(error) = persist_installed_version(&app, &release.version) {
                    set_failed_state(&app, error);
                    return;
                }
                if let Ok(mut slot) = PENDING_RELEASE.lock() {
                    *slot = None;
                }
                set_state(&app, ServerUpdateState::Idle);
            }
            Err(error) => {
                let _ = crate::app_runtime::start_embedded_server_process(&app);
                set_failed_state(&app, error);
            }
        }
    });
}

pub fn handle_tray_action(app: AppHandle) {
    match current_update_state() {
        ServerUpdateState::Available | ServerUpdateState::Failed => start_server_update(app),
        _ => {}
    }
}

fn configure_context(
    variant: Variant,
    exe_relative: Option<&'static str>,
) -> Option<UpdateContext> {
    let exe_relative = exe_relative?;
    match variant {
        Variant::Standalone => Some(UpdateContext {
            owner: STANDALONE_RELEASE_OWNER,
            repo: STANDALONE_RELEASE_REPO,
            exe_relative,
        }),
        _ => None,
    }
}

fn update_context() -> Result<UpdateContext, String> {
    UPDATE_CONTEXT
        .lock()
        .map_err(|_| "Updater state is unavailable".to_string())?
        .clone()
        .ok_or_else(|| "Standalone server download is not configured for this build".to_string())
}

fn run_update_check(app: &AppHandle) -> Result<(), String> {
    let context = update_context()?;
    let current_version = current_local_version(app);
    println!(
        "Checking GitHub releases for standalone server updates in {}/{} (current version: {})",
        context.owner, context.repo, current_version
    );
    let release = fetch_latest_release(&context)?;

    if is_remote_version_newer(&release.version, &current_version) {
        println!(
            "Standalone server update available: {} -> {}",
            current_version, release.version
        );
        if let Ok(mut slot) = PENDING_RELEASE.lock() {
            *slot = Some(release);
        }
        set_state(app, ServerUpdateState::Available);
    } else {
        println!(
            "Standalone server is up to date at version {}",
            current_version
        );
        if let Ok(mut slot) = PENDING_RELEASE.lock() {
            *slot = None;
        }
        set_state(app, ServerUpdateState::Idle);
    }

    Ok(())
}

fn install_release(
    app: &AppHandle,
    context: &UpdateContext,
    release: &ResolvedServerRelease,
    restart_server: bool,
) -> Result<(), String> {
    let target_path = resolve_target_path(app, context)?;
    let temp_root = updater_temp_dir(app)?;
    fs::create_dir_all(&temp_root).map_err(|error| error.to_string())?;

    let staged_asset_path = temp_root.join(&release.asset_name);
    let staged_exe_path = temp_root.join(&release.exe_name);

    println!(
        "Downloading standalone server version {} from {}",
        release.version, release.url
    );
    download_release_asset(release, &staged_asset_path)?;
    println!(
        "Verifying standalone server checksum for {}",
        staged_asset_path.display()
    );
    verify_file_hash(&staged_asset_path, &release.sha256)?;
    println!(
        "Extracting standalone server archive {}",
        staged_asset_path.display()
    );
    extract_7z_archive(
        &staged_asset_path,
        &temp_root,
        &release.exe_name,
        &staged_exe_path,
    )?;
    set_state(app, ServerUpdateState::Installing);
    println!("Installing standalone server to {}", target_path.display());

    if restart_server {
        crate::app_runtime::stop_embedded_server_process();
    }

    if let Err(error) = replace_target_binary(&target_path, &staged_exe_path) {
        if restart_server {
            let _ = crate::app_runtime::start_embedded_server_process(app);
        }
        return Err(error);
    }

    if restart_server {
        crate::app_runtime::start_embedded_server_process(app)?;
    }

    println!(
        "Standalone server version {} installed successfully",
        release.version
    );
    Ok(())
}

fn fetch_latest_release(context: &UpdateContext) -> Result<ResolvedServerRelease, String> {
    let url = format!(
        "{}/repos/{}/{}/releases/latest",
        GITHUB_API_BASE, context.owner, context.repo
    );
    println!(
        "Fetching latest standalone server release metadata from {}",
        url
    );
    let client = github_client()?;
    let release = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Failed to fetch latest GitHub release: {error}"))?
        .json::<GitHubReleaseResponse>()
        .map_err(|error| format!("Failed to parse latest GitHub release response: {error}"))?;

    resolve_release_asset(context, release)
}
fn resolve_release_asset(
    context: &UpdateContext,
    release: GitHubReleaseResponse,
) -> Result<ResolvedServerRelease, String> {
    if release.draft || release.prerelease {
        return Err("Latest GitHub release is a draft or prerelease".to_string());
    }

    let expected_name = Path::new(context.exe_relative)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Failed to resolve server executable name".to_string())?;
    let version = normalize_release_version(&release.tag_name);

    let assets = release.assets;
    let asset = assets
        .iter()
        .find(|asset| asset_matches(asset))
        .cloned()
        .ok_or_else(|| "No .7z server asset found in the latest release".to_string())?;

    let digest = asset
        .digest
        .as_deref()
        .and_then(parse_github_digest)
        .ok_or_else(|| "Release asset is missing a sha256 digest".to_string())?;

    println!(
        "Selected standalone server release {} with asset {}",
        version, asset.name
    );

    Ok(ResolvedServerRelease {
        version,
        url: asset.browser_download_url,
        sha256: digest.to_string(),
        asset_name: asset.name,
        exe_name: expected_name.to_string(),
    })
}

fn asset_matches(asset: &GitHubReleaseAsset) -> bool {
    let lower_name = asset.name.to_ascii_lowercase();
    lower_name.ends_with(".7z") && !asset.browser_download_url.trim().is_empty()
}

fn parse_github_digest(digest: &str) -> Option<&str> {
    digest.strip_prefix("sha256:")
}

fn normalize_release_version(tag_name: &str) -> String {
    tag_name
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string()
}

fn github_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| error.to_string())
}

fn current_local_version(app: &AppHandle) -> String {
    read_persisted_version(app).unwrap_or_else(|| "0.0.0".to_string())
}

fn read_persisted_version(app: &AppHandle) -> Option<String> {
    let path = persisted_version_path(app).ok()?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str::<PersistedServerUpdate>(&contents)
        .ok()
        .map(|record| record.installed_version)
}

fn persist_installed_version(app: &AppHandle, version: &str) -> Result<(), String> {
    let path = persisted_version_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let payload = PersistedServerUpdate {
        installed_version: version.to_string(),
    };
    let json = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

fn persisted_version_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join(SERVER_UPDATE_FILE))
}

fn resolve_target_path(app: &AppHandle, context: &UpdateContext) -> Result<PathBuf, String> {
    managed_server_exe_path(app, context.exe_relative)
        .ok_or_else(|| "Failed to resolve downloaded server path".to_string())
}

fn updater_temp_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("server-update-temp"))
}

fn local_dev_server_path(context: &UpdateContext) -> Option<PathBuf> {
    let path = std::env::current_dir().ok()?.join(context.exe_relative);
    if is_local_server_exe(&path) {
        Some(path)
    } else {
        None
    }
}

fn is_local_server_exe(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("exe"))
            .unwrap_or(false)
}

fn download_release_asset(
    release: &ResolvedServerRelease,
    destination_path: &Path,
) -> Result<(), String> {
    let client = github_client()?;

    let mut response = client
        .get(&release.url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Failed to download server executable: {error}"))?;

    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let mut file = File::create(destination_path).map_err(|error| error.to_string())?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| format!("Failed to download server asset: {error}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| format!("Failed to write server asset: {error}"))?;
    }
    file.flush().map_err(|error| error.to_string())
}

fn extract_7z_archive(
    archive_path: &Path,
    extraction_dir: &Path,
    exe_name: &str,
    staged_exe_path: &Path,
) -> Result<(), String> {
    let archive_output_dir = extraction_dir.join("extracted");
    if archive_output_dir.exists() {
        fs::remove_dir_all(&archive_output_dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&archive_output_dir).map_err(|error| error.to_string())?;

    let extraction_result = sevenz_rust2::decompress_file(archive_path, &archive_output_dir)
        .map_err(|error| format!("Failed to extract 7z server archive: {error}"));
    let _ = fs::remove_file(archive_path);
    extraction_result?;

    let extracted_path = find_file_recursive(&archive_output_dir, exe_name)
        .ok_or_else(|| format!("Extracted 7z archive does not contain '{}'", exe_name))?;

    if staged_exe_path.exists() {
        let _ = fs::remove_file(staged_exe_path);
    }
    fs::rename(&extracted_path, staged_exe_path)
        .map_err(|error| format!("Failed to stage extracted server executable: {error}"))?;
    let _ = fs::remove_dir_all(&archive_output_dir);
    Ok(())
}

fn find_file_recursive(root: &Path, file_name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, file_name) {
                return Some(found);
            }
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case(file_name))
            .unwrap_or(false)
        {
            return Some(path);
        }
    }
    None
}

fn verify_file_hash(file_path: &Path, expected_sha256: &str) -> Result<(), String> {
    let mut file = File::open(file_path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let actual = hex::encode(hasher.finalize());
    if actual.eq_ignore_ascii_case(expected_sha256.trim()) {
        Ok(())
    } else {
        Err(
            "Downloaded server executable checksum does not match the GitHub asset digest"
                .to_string(),
        )
    }
}

fn replace_target_binary(target_path: &Path, staged_exe_path: &Path) -> Result<(), String> {
    if !staged_exe_path.exists() {
        return Err("Downloaded server executable is missing from staging".to_string());
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let backup_path = target_path.with_extension("exe.bak");
    let _ = fs::remove_file(&backup_path);

    let had_existing_target = target_path.exists();
    if had_existing_target {
        fs::rename(target_path, &backup_path).map_err(|error| {
            format!("Failed to move the current server executable out of the way: {error}")
        })?;
    }

    if let Err(error) = fs::rename(staged_exe_path, target_path) {
        if had_existing_target {
            let _ = fs::rename(&backup_path, target_path);
        }
        return Err(format!(
            "Failed to replace the local server executable: {error}"
        ));
    }

    let _ = fs::remove_file(&backup_path);
    Ok(())
}

fn set_failed_state(app: &AppHandle, message: String) {
    eprintln!("Server update failed: {message}");
    set_state(app, ServerUpdateState::Failed);
}

fn set_state(app: &AppHandle, next_state: ServerUpdateState) {
    if let Ok(mut state) = UPDATE_STATE.lock() {
        *state = next_state;
    }
    crate::modules::menu::sync_server_update_menu(app);
}

fn begin_transition_to_checking() -> bool {
    if let Ok(mut state) = UPDATE_STATE.lock() {
        match &*state {
            ServerUpdateState::Downloading
            | ServerUpdateState::Installing
            | ServerUpdateState::Checking => false,
            _ => {
                *state = ServerUpdateState::Checking;
                true
            }
        }
    } else {
        false
    }
}

fn begin_transition_to_downloading(app: &AppHandle) -> bool {
    let changed = if let Ok(mut state) = UPDATE_STATE.lock() {
        match &*state {
            ServerUpdateState::Available | ServerUpdateState::Failed => {
                *state = ServerUpdateState::Downloading;
                true
            }
            _ => false,
        }
    } else {
        false
    };

    if changed {
        crate::modules::menu::sync_server_update_menu(app);
    }

    changed
}

fn is_remote_version_newer(remote: &str, local: &str) -> bool {
    match (
        Version::parse(
            remote
                .trim_start_matches('v')
                .trim_start_matches('V')
                .trim(),
        ),
        Version::parse(local.trim_start_matches('v').trim_start_matches('V').trim()),
    ) {
        (Ok(remote_version), Ok(local_version)) => remote_version > local_version,
        _ => remote.trim() != local.trim(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        asset_matches, is_remote_version_newer, normalize_release_version, GitHubReleaseAsset,
    };

    #[test]
    fn semver_comparison_prefers_newer_remote() {
        assert!(is_remote_version_newer("v1.2.0", "1.1.9"));
        assert!(!is_remote_version_newer("1.2.0", "v1.2.0"));
    }

    #[test]
    fn normalizes_v_prefix() {
        assert_eq!(normalize_release_version("v1.0.0"), "1.0.0");
    }

    #[test]
    fn matches_expected_release_asset() {
        let asset = GitHubReleaseAsset {
            name: "lyrics-smtc-v1.0.0-x64.7z".to_string(),
            browser_download_url: "https://example.com/file.7z".to_string(),
            digest: Some("sha256:abc".to_string()),
        };
        assert!(asset_matches(&asset));
    }

    #[test]
    fn rejects_non_7z_release_asset() {
        let asset = GitHubReleaseAsset {
            name: "lyrics-smtc-v1.0.0-x64.exe".to_string(),
            browser_download_url: "https://example.com/file.exe".to_string(),
            digest: Some("sha256:abc".to_string()),
        };
        assert!(!asset_matches(&asset));
    }
}
