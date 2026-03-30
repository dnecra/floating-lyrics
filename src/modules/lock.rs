use std::env;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

lazy_static::lazy_static! {
    static ref APP_LOCK_FILE: Mutex<Option<File>> = Mutex::new(None);
}

// Acquire an exclusive file lock in the OS temp directory to ensure a single instance.
#[allow(dead_code)]
pub fn acquire_app_lock() -> bool {
    let lock_path = get_lock_path();

    // Try to create the file exclusively
    // On Windows, this uses FILE_SHARE_NONE which prevents other processes from opening
    // On Unix, we rely on the fact that if the file exists, another instance is running
    #[cfg(windows)]
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(0) // FILE_SHARE_NONE - no sharing allowed
        .open(&lock_path);

    #[cfg(unix)]
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true) // Fails if file exists
        .open(&lock_path);

    match file {
        Ok(f) => {
            if let Ok(mut slot) = APP_LOCK_FILE.lock() {
                *slot = Some(f);
                true
            } else {
                false
            }
        }
        Err(e) => {
            eprintln!(
                "Failed to acquire lock (another instance may be running): {}",
                e
            );
            false
        }
    }
}

pub fn release_app_lock() {
    if let Ok(mut slot) = APP_LOCK_FILE.lock() {
        slot.take();
    }
}

fn get_lock_path() -> PathBuf {
    let mut lock_path = env::temp_dir();
    lock_path.push("floating-lyrics.lock");
    lock_path
}
