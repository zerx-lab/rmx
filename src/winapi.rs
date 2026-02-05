use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::core::PWSTR;
#[cfg(windows)]
use windows::Wdk::Storage::FileSystem::{
    FILE_DISPOSITION_DELETE, FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE,
    FILE_DISPOSITION_INFORMATION_EX, FILE_DISPOSITION_INFORMATION_EX_FLAGS,
    FILE_DISPOSITION_POSIX_SEMANTICS,
};
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::Foundation::{ERROR_MORE_DATA, WIN32_ERROR};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FileDispositionInfoEx, FindClose, FindFirstFileExW, FindNextFileW,
    GetFileAttributesW, SetFileInformationByHandle, DELETE, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FINDEX_INFO_LEVELS, FINDEX_SEARCH_OPS,
    FIND_FIRST_EX_FLAGS, INVALID_FILE_ATTRIBUTES, OPEN_EXISTING, WIN32_FIND_DATAW,
};
#[cfg(windows)]
use windows::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY,
    RM_PROCESS_INFO,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, PROCESS_QUERY_INFORMATION, PROCESS_TERMINATE,
};

const MAX_RETRIES: u32 = 5;
const RETRY_DELAYS_MS: [u64; 5] = [0, 50, 100, 200, 500];

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

    for (i, &delay_ms) in RETRY_DELAYS_MS
        .iter()
        .enumerate()
        .take(MAX_RETRIES as usize)
    {
        match unsafe { posix_delete_file(&wide_path) } {
            Ok(()) => return Ok(()),
            Err(e) => {
                if !is_retryable_error(e.raw_os_error().unwrap_or(0)) {
                    return Err(e);
                }
                last_error = Some(e);
                if i < MAX_RETRIES as usize - 1 && delay_ms > 0 {
                    thread::sleep(Duration::from_millis(delay_ms));
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| io::Error::new(io::ErrorKind::Other, "max retries exceeded")))
}

#[cfg(windows)]
pub fn remove_dir(path: &Path) -> io::Result<()> {
    let wide_path = path_to_wide(path);
    let mut last_error = None;

    for (i, &delay_ms) in RETRY_DELAYS_MS
        .iter()
        .enumerate()
        .take(MAX_RETRIES as usize)
    {
        match unsafe { posix_delete_dir(&wide_path) } {
            Ok(()) => return Ok(()),
            Err(e) => {
                if !is_retryable_error(e.raw_os_error().unwrap_or(0)) {
                    return Err(e);
                }
                last_error = Some(e);
                if i < MAX_RETRIES as usize - 1 && delay_ms > 0 {
                    thread::sleep(Duration::from_millis(delay_ms));
                }
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

/// File entry information returned during enumeration
pub struct FileEntry {
    pub path: std::path::PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
}

#[cfg(windows)]
pub fn enumerate_files<F>(dir: &Path, mut callback: F) -> io::Result<()>
where
    F: FnMut(FileEntry) -> io::Result<()>,
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
            Err(_) => {
                let err = io::Error::last_os_error();
                match err.raw_os_error() {
                    Some(2) | Some(3) => return Ok(()), // FILE_NOT_FOUND / PATH_NOT_FOUND (broken symlink)
                    _ => return Err(err),
                }
            }
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
                let is_symlink = (find_data.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0;
                let size = if is_dir {
                    0
                } else {
                    ((find_data.nFileSizeHigh as u64) << 32) | (find_data.nFileSizeLow as u64)
                };
                let full_path = dir.join(&filename);
                callback(FileEntry {
                    path: full_path,
                    is_dir,
                    is_symlink,
                    size,
                })?;
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
    F: FnMut(FileEntry) -> io::Result<()>,
{
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let is_dir = file_type.is_dir();
        let is_symlink = file_type.is_symlink();
        let size = if is_dir || is_symlink {
            0
        } else {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        };
        callback(FileEntry {
            path,
            is_dir,
            is_symlink,
            size,
        })?;
    }
    Ok(())
}

/// Information about a process holding a file lock
#[derive(Debug, Clone)]
pub struct LockingProcess {
    pub pid: u32,
    pub name: String,
}

#[cfg(windows)]
pub fn find_locking_processes(path: &Path) -> io::Result<Vec<LockingProcess>> {
    let wide_path = path_to_wide(path);
    let mut session_handle: u32 = 0;
    let mut session_key = [0u16; CCH_RM_SESSION_KEY as usize + 1];

    let result = unsafe { RmStartSession(&mut session_handle, 0, PWSTR(session_key.as_mut_ptr())) };

    if result != WIN32_ERROR(0) {
        unsafe {
            let _ = RmEndSession(session_handle);
        }
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }

    let file_path_ptr = PCWSTR(wide_path.as_ptr());
    let result = unsafe { RmRegisterResources(session_handle, Some(&[file_path_ptr]), None, None) };

    if result != WIN32_ERROR(0) {
        unsafe {
            let _ = RmEndSession(session_handle);
        }
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }

    let mut proc_info_needed: u32 = 0;
    let mut proc_info_count: u32 = 0;
    let mut reboot_reasons: u32 = 0;

    let result = unsafe {
        RmGetList(
            session_handle,
            &mut proc_info_needed,
            &mut proc_info_count,
            None,
            &mut reboot_reasons,
        )
    };

    if result != WIN32_ERROR(0) && result != ERROR_MORE_DATA {
        unsafe {
            let _ = RmEndSession(session_handle);
        }
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }

    let mut processes = Vec::new();

    if proc_info_needed > 0 {
        let mut proc_info: Vec<RM_PROCESS_INFO> =
            vec![unsafe { std::mem::zeroed() }; proc_info_needed as usize];
        proc_info_count = proc_info_needed;

        let result = unsafe {
            RmGetList(
                session_handle,
                &mut proc_info_needed,
                &mut proc_info_count,
                Some(proc_info.as_mut_ptr()),
                &mut reboot_reasons,
            )
        };

        if result == WIN32_ERROR(0) {
            for i in 0..proc_info_count as usize {
                let info = &proc_info[i];
                let pid = info.Process.dwProcessId;

                let name_len = info
                    .strAppName
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(info.strAppName.len());
                let name = String::from_utf16_lossy(&info.strAppName[..name_len]);

                processes.push(LockingProcess { pid, name });
            }
        }
    }

    unsafe {
        let _ = RmEndSession(session_handle);
    }
    Ok(processes)
}

#[cfg(not(windows))]
pub fn find_locking_processes(_path: &Path) -> io::Result<Vec<LockingProcess>> {
    Ok(Vec::new())
}

#[cfg(windows)]
pub fn find_locking_processes_batch(paths: &[PathBuf]) -> io::Result<Vec<LockingProcess>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let wide_paths: Vec<Vec<u16>> = paths.iter().map(|p| path_to_wide(p)).collect();
    let mut session_handle: u32 = 0;
    let mut session_key = [0u16; CCH_RM_SESSION_KEY as usize + 1];

    let result = unsafe { RmStartSession(&mut session_handle, 0, PWSTR(session_key.as_mut_ptr())) };
    if result != WIN32_ERROR(0) {
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }

    let file_ptrs: Vec<PCWSTR> = wide_paths.iter().map(|p| PCWSTR(p.as_ptr())).collect();
    let result = unsafe { RmRegisterResources(session_handle, Some(&file_ptrs), None, None) };
    if result != WIN32_ERROR(0) {
        unsafe {
            let _ = RmEndSession(session_handle);
        }
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }

    let mut proc_info_needed: u32 = 0;
    let mut proc_info_count: u32 = 0;
    let mut reboot_reasons: u32 = 0;

    let result = unsafe {
        RmGetList(
            session_handle,
            &mut proc_info_needed,
            &mut proc_info_count,
            None,
            &mut reboot_reasons,
        )
    };

    if result != WIN32_ERROR(0) && result != ERROR_MORE_DATA {
        unsafe {
            let _ = RmEndSession(session_handle);
        }
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }

    let mut processes = Vec::new();
    if proc_info_needed > 0 {
        let mut proc_info: Vec<RM_PROCESS_INFO> =
            vec![unsafe { std::mem::zeroed() }; proc_info_needed as usize];
        proc_info_count = proc_info_needed;

        let result = unsafe {
            RmGetList(
                session_handle,
                &mut proc_info_needed,
                &mut proc_info_count,
                Some(proc_info.as_mut_ptr()),
                &mut reboot_reasons,
            )
        };

        if result == WIN32_ERROR(0) {
            for i in 0..proc_info_count as usize {
                let info = &proc_info[i];
                let pid = info.Process.dwProcessId;
                let name_len = info
                    .strAppName
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(info.strAppName.len());
                let name = String::from_utf16_lossy(&info.strAppName[..name_len]);
                processes.push(LockingProcess { pid, name });
            }
        }
    }

    unsafe {
        let _ = RmEndSession(session_handle);
    }
    Ok(processes)
}

#[cfg(not(windows))]
pub fn find_locking_processes_batch(_paths: &[PathBuf]) -> io::Result<Vec<LockingProcess>> {
    Ok(Vec::new())
}

#[cfg(windows)]
pub fn kill_locking_processes_batch(
    paths: &[PathBuf],
    verbose: bool,
) -> io::Result<Vec<LockingProcess>> {
    let processes = find_locking_processes_batch(paths)?;
    let mut killed = Vec::new();

    for proc in &processes {
        if proc.pid == 0 || proc.pid == 4 {
            if verbose {
                eprintln!(
                    "Warning: Skipping system process {} (PID {})",
                    proc.name, proc.pid
                );
            }
            continue;
        }

        match kill_process(proc.pid) {
            Ok(()) => {
                if verbose {
                    eprintln!("Killed process '{}' (PID {})", proc.name, proc.pid);
                }
                killed.push(proc.clone());
            }
            Err(e) => {
                if verbose {
                    eprintln!(
                        "Warning: Failed to kill '{}' (PID {}): {}",
                        proc.name, proc.pid, e
                    );
                }
            }
        }
    }

    if !killed.is_empty() {
        thread::sleep(Duration::from_millis(50));
    }

    Ok(killed)
}

#[cfg(not(windows))]
pub fn kill_locking_processes_batch(
    _paths: &[PathBuf],
    _verbose: bool,
) -> io::Result<Vec<LockingProcess>> {
    Ok(Vec::new())
}

/// Kill a process by PID
#[cfg(windows)]
pub fn kill_process(pid: u32) -> io::Result<()> {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_INFORMATION, false, pid)
            .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;

        let result = TerminateProcess(handle, 1);
        CloseHandle(handle).ok();

        result.map_err(|e| io::Error::from_raw_os_error((e.code().0 & 0xFFFF) as i32))
    }
}

#[cfg(not(windows))]
pub fn kill_process(_pid: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Not supported on this platform",
    ))
}

/// Kill all processes locking a file
#[cfg(windows)]
pub fn kill_locking_processes(path: &Path, verbose: bool) -> io::Result<Vec<LockingProcess>> {
    let processes = find_locking_processes(path)?;
    let mut killed = Vec::new();

    for proc in &processes {
        // Skip system-critical processes (PID 0 and 4 are System processes)
        if proc.pid == 0 || proc.pid == 4 {
            if verbose {
                eprintln!(
                    "Warning: Skipping system process {} (PID {})",
                    proc.name, proc.pid
                );
            }
            continue;
        }

        match kill_process(proc.pid) {
            Ok(()) => {
                if verbose {
                    eprintln!("Killed process '{}' (PID {})", proc.name, proc.pid);
                }
                killed.push(proc.clone());
            }
            Err(e) => {
                if verbose {
                    eprintln!(
                        "Warning: Failed to kill '{}' (PID {}): {}",
                        proc.name, proc.pid, e
                    );
                }
            }
        }
    }

    if !killed.is_empty() {
        thread::sleep(Duration::from_millis(50));
    }

    Ok(killed)
}

#[cfg(not(windows))]
pub fn kill_locking_processes(_path: &Path, _verbose: bool) -> io::Result<Vec<LockingProcess>> {
    Ok(Vec::new())
}

/// Check if an error is a sharing/lock violation (file in use)
/// ERROR_ACCESS_DENIED (5) is included because Windows often returns this
/// when a file is locked by another process, not just for permission issues.
pub fn is_file_in_use_error(error: &io::Error) -> bool {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    matches!(
        error.raw_os_error(),
        Some(ERROR_ACCESS_DENIED) | Some(ERROR_SHARING_VIOLATION) | Some(ERROR_LOCK_VIOLATION)
    )
}

/// Check if an error indicates the file/directory no longer exists
pub fn is_not_found_error(error: &io::Error) -> bool {
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_PATH_NOT_FOUND: i32 = 3;
    const ERROR_INVALID_NAME: i32 = 123;
    const ERROR_BAD_PATHNAME: i32 = 161;

    if let Some(code) = error.raw_os_error() {
        if matches!(
            code,
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_INVALID_NAME | ERROR_BAD_PATHNAME
        ) {
            return true;
        }
    }

    error.kind() == io::ErrorKind::NotFound
}
