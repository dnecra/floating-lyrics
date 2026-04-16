use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

use crate::app_runtime::Variant;

const UPDATE_CHECK_DELAY_SECS: u64 = 10;
const SERVER_UPDATE_FILE: &str = "server-update.json";
const RELEASE_CACHE_FILE: &str = "server-release-cache.json";
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
const GITHUB_CONNECT_TIMEOUT_SECS: u64 = 10;
const GITHUB_REQUEST_TIMEOUT_SECS: u64 = 180;
const RELEASE_FETCH_RETRIES: usize = 5;
const RELEASE_DOWNLOAD_RETRIES: usize = 5;
const RETRY_DELAY_SECS: u64 = 2;
const RELEASE_CACHE_TTL_SECS: u64 = 60 * 60 * 24 * 7; // 7 days — background checker handles freshness

lazy_static::lazy_static! {
    static ref UPDATE_CONTEXT: Mutex<Option<UpdateContext>> = Mutex::new(None);
    static ref UPDATE_STATE: Mutex<ServerUpdateState> = Mutex::new(ServerUpdateState::Idle);
    static ref LATEST_RELEASE: Mutex<Option<ResolvedServerRelease>> = Mutex::new(None);
    static ref INSTALLED_RELEASE_TAG: Mutex<Option<String>> = Mutex::new(None);
}

#[derive(Clone)]
struct UpdateContext {
    owner: &'static str,
    repo: &'static str,
    exe_relative: &'static str,
    asset_kind: ReleaseAssetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseAssetKind {
    Archive7z,
    Executable,
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
    tag: String,
    url: String,
    sha256: String,
    asset_name: String,
    exe_name: String,
    asset_kind: ReleaseAssetKind,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PersistedLatestRelease {
    fetched_at_unix_secs: u64,
    tag: String,
    url: String,
    sha256: String,
    asset_name: String,
    exe_name: String,
    asset_kind: PersistedReleaseAssetKind,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
enum PersistedReleaseAssetKind {
    Archive7z,
    Executable,
}

#[derive(Debug, Clone)]
pub enum ServerUpdateState {
    Idle,
    ResolvingLatest,
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
    hydrate_installed_release_tag(&app);
    hydrate_cached_release(&app);

    crate::modules::menu::sync_server_update_menu(&app);

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(UPDATE_CHECK_DELAY_SECS));
        start_update_check(app);
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
    hydrate_installed_release_tag(app);
    hydrate_cached_release(app);

    let target_path = resolve_target_path(app, &context)?;
    let installed_tag = cached_installed_tag();
    let has_managed_binary = target_path.is_file();

    // If the binary is already installed and the persisted cache agrees it's current,
    // skip the blocking API call entirely on startup. The background update check
    // in initialize() runs 10 seconds later and will fetch + install any newer release.
    if has_managed_binary {
        if let Ok(Some(cached)) = read_cached_release(app) {
            if installed_tag.as_deref() == Some(cached.tag.as_str()) {
                println!(
                    "Standalone server {} already installed and cache is fresh, skipping API call on startup",
                    cached.tag
                );
                set_state(app, ServerUpdateState::Idle);
                return Ok(());
            }
        }
    }

    // Binary is missing or installed tag doesn't match cache — hit the API.
    set_state(app, ServerUpdateState::ResolvingLatest);
    let release = fetch_latest_release_with_retry(app, &context, false)?;
    cache_latest_release(app, release.clone());

    if has_managed_binary && installed_tag.as_deref() == Some(release.tag.as_str()) {
        println!(
            "Standalone server already installed at {} and up to date with {}",
            target_path.display(),
            release.tag
        );
    } else {
        println!(
            "Preparing standalone server from the latest GitHub release for {}/{}...",
            context.owner, context.repo
        );
        set_state(app, ServerUpdateState::Downloading);
        install_release(app, &context, &release, false)?;
        persist_installed_version(app, &release.tag)?;
        println!(
            "Standalone server bootstrap complete. Installed version {}",
            release.tag
        );
    }
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
    if update_context().is_err() {
        return None;
    }

    match current_update_state() {
        ServerUpdateState::Idle => Some(idle_tray_descriptor()),
        ServerUpdateState::ResolvingLatest => {
            Some(("Resolving latest server release...".to_string(), false))
        }
        ServerUpdateState::Downloading => {
            let version_suffix = display_tag_suffix();
            Some((format!("Downloading server{version_suffix}..."), false))
        }
        ServerUpdateState::Installing => {
            let version_suffix = display_tag_suffix();
            Some((format!("Installing server{version_suffix}..."), false))
        }
        ServerUpdateState::Failed => {
            let version_suffix = display_tag_suffix();
            Some((
                format!("Retry latest server download{version_suffix}"),
                true,
            ))
        }
    }
}

pub fn start_update_check(app: AppHandle) {
    if !begin_transition_to_resolving() {
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
    if !begin_transition_to_downloading(&app) {
        return;
    }

    thread::spawn(move || {
        let result = update_context().and_then(|context| {
            set_state(&app, ServerUpdateState::ResolvingLatest);
            let release = fetch_latest_release_with_retry(&app, &context, true)?;
            cache_latest_release(&app, release.clone());
            set_state(&app, ServerUpdateState::Downloading);
            install_release(&app, &context, &release, true)?;
            persist_installed_version(&app, &release.tag)?;
            Ok(())
        });

        match result {
            Ok(()) => set_state(&app, ServerUpdateState::Idle),
            Err(error) => {
                let _ = crate::app_runtime::start_embedded_server_process(&app);
                set_failed_state(&app, error);
            }
        }
    });
}

pub fn handle_tray_action(app: AppHandle) {
    match current_update_state() {
        ServerUpdateState::Idle => {
            if has_newer_latest_release() {
                start_server_update(app);
            }
        }
        ServerUpdateState::Failed => start_server_update(app),
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
            asset_kind: ReleaseAssetKind::Archive7z,
        }),
        Variant::Ytm => Some(UpdateContext {
            owner: STANDALONE_RELEASE_OWNER,
            repo: STANDALONE_RELEASE_REPO,
            exe_relative,
            asset_kind: ReleaseAssetKind::Executable,
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
    if cached_release_is_fresh(app) {
        set_state(app, ServerUpdateState::Idle);
        return Ok(());
    }
    println!(
        "Resolving latest standalone server release in {}/{}",
        context.owner, context.repo
    );
    let release = fetch_latest_release_with_retry(app, &context, false)?;
    println!("Latest standalone server release is {}", release.tag);
    cache_latest_release(app, release);
    set_state(app, ServerUpdateState::Idle);
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

    let staged_exe_path = temp_root.join(&release.exe_name);
    let staged_asset_path = temp_root.join(&release.asset_name);

    println!(
        "Downloading standalone server version {} from {}",
        release.tag, release.url
    );
    match release.asset_kind {
        ReleaseAssetKind::Archive7z => {
            download_release_asset_with_retry(release, &staged_asset_path)?;
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
        }
        ReleaseAssetKind::Executable => {
            download_release_asset_with_retry(release, &staged_exe_path)?;
            println!(
                "Verifying standalone server checksum for {}",
                staged_exe_path.display()
            );
            verify_file_hash(&staged_exe_path, &release.sha256)?;
        }
    }
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
        release.tag
    );
    Ok(())
}

fn fetch_latest_release_with_retry(
    app: &AppHandle,
    context: &UpdateContext,
    force_refresh: bool,
) -> Result<ResolvedServerRelease, String> {
    if !force_refresh {
        if let Some(release) = read_cached_release(app)? {
            return Ok(release);
        }
    }

    retry_with_backoff(
        RELEASE_FETCH_RETRIES,
        "fetch latest GitHub release metadata",
        || fetch_latest_release(context),
    )
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
    let tag = normalize_release_tag(&release.tag_name);

    let asset = release
        .assets
        .iter()
        .find(|asset| asset_matches(asset, context.asset_kind, expected_name))
        .cloned()
        .ok_or_else(|| {
            format!(
                "No matching {} server asset found in the latest release",
                context.asset_kind.extension_label()
            )
        })?;

    let digest = asset
        .digest
        .as_deref()
        .and_then(parse_github_digest)
        .ok_or_else(|| "Release asset is missing a sha256 digest".to_string())?;

    println!(
        "Selected standalone server release {} with asset {}",
        tag, asset.name
    );

    Ok(ResolvedServerRelease {
        tag,
        url: asset.browser_download_url,
        sha256: digest.to_string(),
        asset_name: asset.name,
        exe_name: expected_name.to_string(),
        asset_kind: context.asset_kind,
    })
}

impl ReleaseAssetKind {
    fn extension_label(self) -> &'static str {
        match self {
            Self::Archive7z => ".7z",
            Self::Executable => ".exe",
        }
    }
}

impl PersistedLatestRelease {
    fn from_release(release: &ResolvedServerRelease) -> Self {
        Self {
            fetched_at_unix_secs: current_unix_secs(),
            tag: release.tag.clone(),
            url: release.url.clone(),
            sha256: release.sha256.clone(),
            asset_name: release.asset_name.clone(),
            exe_name: release.exe_name.clone(),
            asset_kind: release.asset_kind.into(),
        }
    }
}

impl From<PersistedLatestRelease> for ResolvedServerRelease {
    fn from(value: PersistedLatestRelease) -> Self {
        Self {
            tag: value.tag,
            url: value.url,
            sha256: value.sha256,
            asset_name: value.asset_name,
            exe_name: value.exe_name,
            asset_kind: value.asset_kind.into(),
        }
    }
}

impl From<ReleaseAssetKind> for PersistedReleaseAssetKind {
    fn from(value: ReleaseAssetKind) -> Self {
        match value {
            ReleaseAssetKind::Archive7z => Self::Archive7z,
            ReleaseAssetKind::Executable => Self::Executable,
        }
    }
}

impl From<PersistedReleaseAssetKind> for ReleaseAssetKind {
    fn from(value: PersistedReleaseAssetKind) -> Self {
        match value {
            PersistedReleaseAssetKind::Archive7z => Self::Archive7z,
            PersistedReleaseAssetKind::Executable => Self::Executable,
        }
    }
}

fn asset_matches(
    asset: &GitHubReleaseAsset,
    asset_kind: ReleaseAssetKind,
    expected_name: &str,
) -> bool {
    let lower_name = asset.name.to_ascii_lowercase();
    let expected_name = expected_name.to_ascii_lowercase();
    !asset.browser_download_url.trim().is_empty()
        && match asset_kind {
            ReleaseAssetKind::Archive7z => lower_name.ends_with(".7z"),
            ReleaseAssetKind::Executable => lower_name == expected_name,
        }
}

fn parse_github_digest(digest: &str) -> Option<&str> {
    digest.strip_prefix("sha256:")
}

fn normalize_release_tag(tag_name: &str) -> String {
    tag_name.trim().to_string()
}

fn github_client() -> Result<Client, String> {
    let token = std::env::var("GITHUB_TOKEN")
        .map_err(|_| "GITHUB_TOKEN is required for standalone server release downloads".to_string())
        .and_then(|token| {
            let trimmed = token.trim().to_string();
            if trimmed.is_empty() {
                Err("GITHUB_TOKEN is required for standalone server release downloads".to_string())
            } else {
                Ok(trimmed)
            }
        })?;
    let auth_value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|error| {
            format!("GITHUB_TOKEN is invalid for GitHub Authorization header: {error}")
        })?;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::AUTHORIZATION, auth_value);

    // Never bundle a token in release builds — this is for local development only.
    Client::builder()
        .connect_timeout(Duration::from_secs(GITHUB_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(GITHUB_REQUEST_TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()
        .map_err(|error| error.to_string())
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
    fs::write(path, json).map_err(|error| error.to_string())?;
    if let Ok(mut slot) = INSTALLED_RELEASE_TAG.lock() {
        *slot = Some(version.to_string());
    }
    Ok(())
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

fn download_release_asset_with_retry(
    release: &ResolvedServerRelease,
    destination_path: &Path,
) -> Result<(), String> {
    retry_with_backoff(
        RELEASE_DOWNLOAD_RETRIES,
        "download latest server asset",
        || download_release_asset(release, destination_path),
    )
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

    let partial_path = destination_path.with_extension("part");
    let _ = fs::remove_file(&partial_path);

    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let mut file = File::create(&partial_path).map_err(|error| error.to_string())?;
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

    file.flush().map_err(|error| error.to_string())?;
    let _ = fs::remove_file(destination_path);
    fs::rename(&partial_path, destination_path)
        .map_err(|error| format!("Failed to move downloaded server asset into place: {error}"))
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

fn begin_transition_to_resolving() -> bool {
    if let Ok(mut state) = UPDATE_STATE.lock() {
        match &*state {
            ServerUpdateState::Downloading
            | ServerUpdateState::Installing
            | ServerUpdateState::ResolvingLatest => false,
            _ => {
                *state = ServerUpdateState::ResolvingLatest;
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
            ServerUpdateState::Idle | ServerUpdateState::Failed => {
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

fn cache_latest_release(app: &AppHandle, release: ResolvedServerRelease) {
    if let Ok(mut slot) = LATEST_RELEASE.lock() {
        *slot = Some(release.clone());
    }
    let _ = persist_cached_release(app, &release);
}

fn idle_tray_descriptor() -> (String, bool) {
    let installed = cached_installed_tag();
    let latest = cached_release_tag();

    match (installed, latest) {
        (Some(installed), Some(latest)) if installed == latest => {
            (format!("Server {latest}"), false)
        }
        (_, Some(latest)) => (format!("Update server to {latest}"), true),
        (Some(installed), None) => (format!("Server {installed}"), false),
        (None, None) => ("Server version unknown".to_string(), false),
    }
}

fn display_tag_suffix() -> String {
    cached_release_tag()
        .or_else(cached_installed_tag)
        .map(|tag| format!(" ({tag})"))
        .unwrap_or_default()
}

fn has_newer_latest_release() -> bool {
    match (cached_installed_tag(), cached_release_tag()) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(installed), Some(latest)) => installed != latest,
    }
}

fn cached_release_tag() -> Option<String> {
    LATEST_RELEASE
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|release| release.tag.clone()))
}

fn cached_installed_tag() -> Option<String> {
    INSTALLED_RELEASE_TAG
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
}

fn hydrate_installed_release_tag(app: &AppHandle) {
    let installed = read_persisted_version(app);
    if let Ok(mut slot) = INSTALLED_RELEASE_TAG.lock() {
        *slot = installed;
    }
}

fn hydrate_cached_release(app: &AppHandle) {
    let cached = read_cached_release(app).ok().flatten();
    if let Ok(mut slot) = LATEST_RELEASE.lock() {
        *slot = cached;
    }
}

fn cached_release_is_fresh(app: &AppHandle) -> bool {
    read_cached_release(app)
        .map(|cached| cached.is_some())
        .unwrap_or(false)
}

fn read_cached_release(app: &AppHandle) -> Result<Option<ResolvedServerRelease>, String> {
    let path = persisted_release_cache_path(app)?;
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let cached: PersistedLatestRelease =
        serde_json::from_str(&contents).map_err(|error| error.to_string())?;
    if current_unix_secs().saturating_sub(cached.fetched_at_unix_secs) > RELEASE_CACHE_TTL_SECS {
        return Ok(None);
    }
    Ok(Some(cached.into()))
}

fn persist_cached_release(app: &AppHandle, release: &ResolvedServerRelease) -> Result<(), String> {
    let path = persisted_release_cache_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let payload = PersistedLatestRelease::from_release(release);
    let json = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

fn persisted_release_cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join(RELEASE_CACHE_FILE))
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn retry_with_backoff<T, F>(attempts: usize, action: &str, mut operation: F) -> Result<T, String>
where
    F: FnMut() -> Result<T, String>,
{
    let mut last_error = String::new();
    for attempt in 1..=attempts {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => {
                last_error = error;
                eprintln!(
                    "Failed to {} (attempt {}/{}): {}",
                    action, attempt, attempts, last_error
                );
                let lower = last_error.to_ascii_lowercase();
                if lower.contains("rate limit") || lower.contains("403") {
                    break;
                }
                if attempt < attempts {
                    thread::sleep(Duration::from_secs(RETRY_DELAY_SECS * attempt as u64));
                }
            }
        }
    }

    Err(format!(
        "Unable to {} after {} attempts: {}",
        action, attempts, last_error
    ))
}

#[cfg(test)]
mod tests {
    use super::{asset_matches, normalize_release_tag, GitHubReleaseAsset, ReleaseAssetKind};

    #[test]
    fn preserves_release_tag() {
        assert_eq!(normalize_release_tag("v1.0.0"), "v1.0.0");
    }

    #[test]
    fn matches_expected_release_asset() {
        let asset = GitHubReleaseAsset {
            name: "lyrics-smtc-v1.0.0-x64.7z".to_string(),
            browser_download_url: "https://example.com/file.7z".to_string(),
            digest: Some("sha256:abc".to_string()),
        };
        assert!(asset_matches(
            &asset,
            ReleaseAssetKind::Archive7z,
            "lyrics-smtc-x64.exe"
        ));
    }

    #[test]
    fn rejects_non_7z_release_asset() {
        let asset = GitHubReleaseAsset {
            name: "lyrics-smtc-v1.0.0-x64.exe".to_string(),
            browser_download_url: "https://example.com/file.exe".to_string(),
            digest: Some("sha256:abc".to_string()),
        };
        assert!(!asset_matches(
            &asset,
            ReleaseAssetKind::Archive7z,
            "lyrics-smtc-x64.exe"
        ));
    }

    #[test]
    fn matches_expected_executable_asset() {
        let asset = GitHubReleaseAsset {
            name: "lyrics-ytm-x64.exe".to_string(),
            browser_download_url: "https://example.com/file.exe".to_string(),
            digest: Some("sha256:abc".to_string()),
        };
        assert!(asset_matches(
            &asset,
            ReleaseAssetKind::Executable,
            "lyrics-ytm-x64.exe"
        ));
    }

    #[test]
    fn rejects_mismatched_executable_asset() {
        let asset = GitHubReleaseAsset {
            name: "lyrics-smtc-x64.exe".to_string(),
            browser_download_url: "https://example.com/file.exe".to_string(),
            digest: Some("sha256:abc".to_string()),
        };
        assert!(!asset_matches(
            &asset,
            ReleaseAssetKind::Executable,
            "lyrics-ytm-x64.exe"
        ));
    }
}
