use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Wdk::Storage::FileSystem::{
    FILE_DISPOSITION_DELETE, FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE,
    FILE_DISPOSITION_INFORMATION_EX, FILE_DISPOSITION_INFORMATION_EX_FLAGS,
    FILE_DISPOSITION_POSIX_SEMANTICS,
};
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FileDispositionInfoEx, FindClose, FindFirstFileExW, FindNextFileW,
    GetFileAttributesW, SetFileInformationByHandle, DELETE, FILE_ATTRIBUTE_DIRECTORY,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FINDEX_INFO_LEVELS, FINDEX_SEARCH_OPS, FIND_FIRST_EX_FLAGS,
    INVALID_FILE_ATTRIBUTES, OPEN_EXISTING, WIN32_FIND_DATAW,
};

const MAX_RETRIES: u32 = 3;
const INITIAL_RETRY_DELAY_MS: u64 = 1;
const MAX_RETRY_DELAY_MS: u64 = 10;

#[cfg(windows)]
pub fn path_exists(path: &Path) -> bool {
    let wide_path = path_to_wide(path);
    unsafe {
        let attrs = GetFileAttributesW(PCWSTR(wide_path.as_ptr()));
        if attrs != INVALID_FILE_ATTRIBUTES {
            return true;
        }
        path_exists_via_find(path)
    }
}

#[cfg(windows)]
fn path_exists_via_find(path: &Path) -> bool {
    let wide_path = path_to_wide(path);
    unsafe {
        let mut find_data: WIN32_FIND_DATAW = std::mem::zeroed();
        match FindFirstFileExW(
            PCWSTR(wide_path.as_ptr()),
            FINDEX_INFO_LEVELS(0),
            &mut find_data as *mut _ as *mut _,
            FINDEX_SEARCH_OPS(0),
            None,
            FIND_FIRST_EX_FLAGS(0),
        ) {
            Ok(handle) => {
                let _ = FindClose(handle);
                true
            }
            Err(_) => false,
        }
    }
}

#[cfg(windows)]
pub fn is_directory(path: &Path) -> bool {
    let wide_path = path_to_wide(path);
    unsafe {
        let attrs = GetFileAttributesW(PCWSTR(wide_path.as_ptr()));
        if attrs != INVALID_FILE_ATTRIBUTES {
            return (attrs & FILE_ATTRIBUTE_DIRECTORY.0) != 0;
        }
        is_directory_via_find(path)
    }
}

#[cfg(windows)]
fn is_directory_via_find(path: &Path) -> bool {
    let wide_path = path_to_wide(path);
    unsafe {
        let mut find_data: WIN32_FIND_DATAW = std::mem::zeroed();
        match FindFirstFileExW(
            PCWSTR(wide_path.as_ptr()),
            FINDEX_INFO_LEVELS(0),
            &mut find_data as *mut _ as *mut _,
            FINDEX_SEARCH_OPS(0),
            None,
            FIND_FIRST_EX_FLAGS(0),
        ) {
            Ok(handle) => {
                let _ = FindClose(handle);
                (find_data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0) != 0
            }
            Err(_) => false,
        }
    }
}

#[cfg(not(windows))]
pub fn path_exists(path: &Path) -> bool {
    path.exists()
}

#[cfg(not(windows))]
pub fn is_directory(path: &Path) -> bool {
    path.is_dir()
}

#[cfg(windows)]
fn path_to_wide(path: &Path) -> Vec<u16> {
    let path_str = path.to_string_lossy();
    // Normalize forward slashes to backslashes for Windows compatibility
    let normalized = path_str.replace('/', "\\");

    // Check if path is absolute (handles both C:\ and \\?\ formats)
    let is_absolute = normalized.starts_with(r"\\?\")
        || (normalized.len() >= 3
            && normalized.chars().nth(1) == Some(':')
            && normalized.chars().nth(2) == Some('\\'));

    let prefixed = if is_absolute && !normalized.starts_with(r"\\?\") {
        format!(r"\\?\{}", normalized)
    } else {
        normalized
    };
    prefixed.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn is_retryable_error(code: i32) -> bool {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    const ERROR_ACCESS_DENIED: i32 = 5;

    matches!(
        code,
        ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION | ERROR_ACCESS_DENIED
    )
}

#[cfg(windows)]
pub fn delete_file(path: &Path) -> io::Result<()> {
    let wide_path = path_to_wide(path);
    let mut last_error = None;
    let mut delay_ms = INITIAL_RETRY_DELAY_MS;

    for _ in 0..MAX_RETRIES {
        match unsafe { posix_delete_file(&wide_path) } {
            Ok(()) => return Ok(()),
            Err(e) => {
                if !is_retryable_error(e.raw_os_error().unwrap_or(0)) {
                    return Err(e);
                }
                last_error = Some(e);
                thread::sleep(Duration::from_millis(delay_ms));
                delay_ms = (delay_ms * 2).min(MAX_RETRY_DELAY_MS);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| io::Error::new(io::ErrorKind::Other, "max retries exceeded")))
}

#[cfg(windows)]
pub fn remove_dir(path: &Path) -> io::Result<()> {
    let wide_path = path_to_wide(path);
    let mut last_error = None;
    let mut delay_ms = INITIAL_RETRY_DELAY_MS;

    for _ in 0..MAX_RETRIES {
        match unsafe { posix_delete_dir(&wide_path) } {
            Ok(()) => return Ok(()),
            Err(e) => {
                if !is_retryable_error(e.raw_os_error().unwrap_or(0)) {
                    return Err(e);
                }
                last_error = Some(e);
                thread::sleep(Duration::from_millis(delay_ms));
                delay_ms = (delay_ms * 2).min(MAX_RETRY_DELAY_MS);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| io::Error::new(io::ErrorKind::Other, "max retries exceeded")))
}

#[cfg(windows)]
unsafe fn posix_delete_file(wide_path: &[u16]) -> io::Result<()> {
    let handle = CreateFileW(
        PCWSTR(wide_path.as_ptr()),
        DELETE.0,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        None,
        OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT,
        HANDLE::default(),
    )
    .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;

    let mut info = FILE_DISPOSITION_INFORMATION_EX {
        Flags: FILE_DISPOSITION_INFORMATION_EX_FLAGS(
            FILE_DISPOSITION_DELETE.0
                | FILE_DISPOSITION_POSIX_SEMANTICS.0
                | FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE.0,
        ),
    };

    let result = SetFileInformationByHandle(
        handle,
        FileDispositionInfoEx,
        &mut info as *mut _ as *mut _,
        std::mem::size_of::<FILE_DISPOSITION_INFORMATION_EX>() as u32,
    );

    CloseHandle(handle).ok();

    result.map_err(|e| io::Error::from_raw_os_error((e.code().0 & 0xFFFF) as i32))
}

#[cfg(windows)]
unsafe fn posix_delete_dir(wide_path: &[u16]) -> io::Result<()> {
    let handle = CreateFileW(
        PCWSTR(wide_path.as_ptr()),
        DELETE.0,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        None,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        HANDLE::default(),
    )
    .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;

    let mut info = FILE_DISPOSITION_INFORMATION_EX {
        Flags: FILE_DISPOSITION_INFORMATION_EX_FLAGS(
            FILE_DISPOSITION_DELETE.0 | FILE_DISPOSITION_POSIX_SEMANTICS.0,
        ),
    };

    let result = SetFileInformationByHandle(
        handle,
        FileDispositionInfoEx,
        &mut info as *mut _ as *mut _,
        std::mem::size_of::<FILE_DISPOSITION_INFORMATION_EX>() as u32,
    );

    CloseHandle(handle).ok();

    result.map_err(|e| io::Error::from_raw_os_error((e.code().0 & 0xFFFF) as i32))
}

#[cfg(not(windows))]
pub fn delete_file(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

#[cfg(not(windows))]
pub fn remove_dir(path: &Path) -> io::Result<()> {
    std::fs::remove_dir(path)
}

#[cfg(windows)]
pub fn enumerate_files<F>(dir: &Path, mut callback: F) -> io::Result<()>
where
    F: FnMut(&Path, bool) -> io::Result<()>,
{
    let search_path = dir.join("*");
    let wide_path = path_to_wide(&search_path);

    unsafe {
        let mut find_data: WIN32_FIND_DATAW = std::mem::zeroed();
        let handle = match FindFirstFileExW(
            PCWSTR(wide_path.as_ptr()),
            FINDEX_INFO_LEVELS(1),
            &mut find_data as *mut _ as *mut _,
            FINDEX_SEARCH_OPS(0),
            None,
            FIND_FIRST_EX_FLAGS(0),
        ) {
            Ok(h) => h,
            Err(_) => return Err(io::Error::last_os_error()),
        };

        loop {
            let name_len = find_data
                .cFileName
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(find_data.cFileName.len());
            let filename = String::from_utf16_lossy(&find_data.cFileName[..name_len]);

            if filename != "." && filename != ".." {
                let is_dir = (find_data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0) != 0;
                let full_path = dir.join(&filename);
                callback(&full_path, is_dir)?;
            }

            if FindNextFileW(handle, &mut find_data).is_err() {
                break;
            }
        }

        let _ = FindClose(handle);
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn enumerate_files<F>(dir: &Path, mut callback: F) -> io::Result<()>
where
    F: FnMut(&Path, bool) -> io::Result<()>,
{
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let is_dir = entry.file_type()?.is_dir();
        callback(&path, is_dir)?;
    }
    Ok(())
}
