use std::fs::File;
use std::sync::Mutex;

#[cfg(not(windows))]
use std::env;
#[cfg(not(windows))]
use std::fs::{remove_file, OpenOptions};
#[cfg(not(windows))]
use std::path::PathBuf;

#[cfg(windows)]
use std::ffi::c_void;

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE};
#[cfg(windows)]
use windows::Win32::System::Threading::CreateMutexW;
#[cfg(windows)]
use windows::core::PCWSTR;

lazy_static::lazy_static! {
    static ref APP_LOCK_FILE: Mutex<Option<File>> = Mutex::new(None);
    #[cfg(windows)]
    static ref APP_MUTEX_HANDLE: Mutex<Option<isize>> = Mutex::new(None);
}

const APP_LOCK_NAME: &str = "Global\\FloatingLyrics.SingleInstance";

#[allow(dead_code)]
pub fn acquire_app_lock() -> bool {
    #[cfg(windows)]
    {
        return acquire_windows_app_lock();
    }

    #[cfg(not(windows))]
    {
        acquire_file_lock()
    }
}

pub fn release_app_lock() {
    #[cfg(windows)]
    release_windows_app_lock();

    #[cfg(not(windows))]
    release_file_lock();
}

#[cfg(windows)]
fn acquire_windows_app_lock() -> bool {
    let name: Vec<u16> = APP_LOCK_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) };

    let Ok(handle) = handle else {
        eprintln!("Failed to create app mutex");
        return false;
    };

    if unsafe { windows::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        eprintln!("Another instance is already holding the app mutex");
        return false;
    }

    if let Ok(mut slot) = APP_MUTEX_HANDLE.lock() {
        *slot = Some(handle.0 as isize);
        true
    } else {
        unsafe {
            let _ = CloseHandle(handle);
        }
        false
    }
}

#[cfg(windows)]
fn release_windows_app_lock() {
    let handle = APP_MUTEX_HANDLE.lock().ok().and_then(|mut slot| slot.take());
    if let Some(raw) = handle {
        unsafe {
            let _ = CloseHandle(HANDLE(raw as *mut c_void));
        }
    }
}

#[cfg(not(windows))]
fn acquire_file_lock() -> bool {
    let lock_path = get_lock_path();

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&lock_path);

    match file {
        Ok(file) => {
            if let Ok(mut slot) = APP_LOCK_FILE.lock() {
                *slot = Some(file);
                true
            } else {
                false
            }
        }
        Err(error) => {
            eprintln!(
                "Failed to acquire app file lock (another instance may be running): {}",
                error
            );
            false
        }
    }
}

#[cfg(not(windows))]
fn release_file_lock() {
    if let Ok(mut slot) = APP_LOCK_FILE.lock() {
        slot.take();
    }
    let _ = remove_file(get_lock_path());
}

#[cfg(not(windows))]
fn get_lock_path() -> PathBuf {
    let mut lock_path = env::temp_dir();
    lock_path.push("floating-lyrics.lock");
    lock_path
}
