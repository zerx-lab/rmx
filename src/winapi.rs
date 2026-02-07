use std::ffi::c_void;
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
    FILE_DISPOSITION_DELETE, FILE_DISPOSITION_FORCE_IMAGE_SECTION_CHECK,
    FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_INFORMATION_EX,
    FILE_DISPOSITION_INFORMATION_EX_FLAGS, FILE_DISPOSITION_POSIX_SEMANTICS,
};
#[cfg(windows)]
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS};
#[cfg(windows)]
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_CLOSE_SOURCE, DUPLICATE_SAME_ACCESS, HANDLE, NTSTATUS,
    STATUS_INFO_LENGTH_MISMATCH,
};
#[cfg(windows)]
use windows::Win32::Foundation::{ERROR_MORE_DATA, WIN32_ERROR};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FileDispositionInfoEx, FindClose, FindFirstFileExW, FindNextFileW,
    GetFileAttributesW, GetFinalPathNameByHandleW, SetFileInformationByHandle, DELETE,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_NAME_NORMALIZED, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FINDEX_INFO_LEVELS, FINDEX_SEARCH_OPS, FIND_FIRST_EX_FLAGS,
    INVALID_FILE_ATTRIBUTES, OPEN_EXISTING, WIN32_FIND_DATAW,
};
#[cfg(windows)]
use windows::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY,
    RM_PROCESS_INFO,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, TerminateProcess, PROCESS_DUP_HANDLE,
    PROCESS_QUERY_INFORMATION, PROCESS_TERMINATE,
};

const MAX_RETRIES: u32 = 4;
const RETRY_DELAYS_MS: [u64; 4] = [0, 1, 5, 10];

/// POSIX delete on hardlinked files (pnpm node_modules) can return Ok() while
/// NTFS directory entry removal is still pending. Passive retry isn't enough —
/// we must actively re-enumerate and re-delete remaining entries.
const DIR_NOT_EMPTY_CLEANUP_ROUNDS: usize = 5;
const DIR_NOT_EMPTY_CLEANUP_DELAYS_MS: [u64; 5] = [1, 10, 50, 100, 200];

#[cfg(windows)]
pub fn path_exists(path: &Path) -> bool {
    let wide_path = path_to_wide(path);
    unsafe {
        let attrs = GetFileAttributesW(PCWSTR(wide_path.as_ptr()));
        if attrs != INVALID_FILE_ATTRIBUTES {
            return true;
        }
        path_exists_via_find_wide(&wide_path)
    }
}

#[cfg(windows)]
fn path_exists_via_find_wide(wide_path: &[u16]) -> bool {
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
        is_directory_via_find_wide(&wide_path)
    }
}

#[cfg(windows)]
fn is_directory_via_find_wide(wide_path: &[u16]) -> bool {
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
    const ERROR_DIR_NOT_EMPTY: i32 = 145;

    matches!(
        code,
        ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION | ERROR_ACCESS_DENIED | ERROR_DIR_NOT_EMPTY
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

    Err(last_error.unwrap_or_else(|| io::Error::other("max retries exceeded")))
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

    if let Some(ref e) = last_error {
        if is_dir_not_empty_error(e) {
            for &delay in DIR_NOT_EMPTY_CLEANUP_DELAYS_MS
                .iter()
                .take(DIR_NOT_EMPTY_CLEANUP_ROUNDS)
            {
                thread::sleep(Duration::from_millis(delay));

                cleanup_remaining_entries(path);

                match unsafe { posix_delete_dir(&wide_path) } {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        if !is_dir_not_empty_error(&e)
                            && !is_retryable_error(e.raw_os_error().unwrap_or(0))
                        {
                            return Err(e);
                        }
                        last_error = Some(e);
                    }
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| io::Error::other("max retries exceeded")))
}

#[cfg(windows)]
fn cleanup_remaining_entries(path: &Path) {
    let _ = enumerate_files(path, |entry| {
        let wide = path_to_wide(&entry.path);
        if entry.is_dir {
            cleanup_remaining_entries(&entry.path);
            let _ = unsafe { posix_delete_dir(&wide) };
        } else {
            let _ = unsafe { posix_delete_file(&wide) };
        }
        Ok(())
    });
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
                | FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE.0
                | FILE_DISPOSITION_FORCE_IMAGE_SECTION_CHECK.0,
        ),
    };

    let result = SetFileInformationByHandle(
        handle,
        FileDispositionInfoEx,
        &mut info as *mut _ as *mut _,
        std::mem::size_of::<FILE_DISPOSITION_INFORMATION_EX>() as u32,
    );

    CloseHandle(handle).ok();

    result.map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
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
            FILE_DISPOSITION_DELETE.0
                | FILE_DISPOSITION_POSIX_SEMANTICS.0
                | FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE.0
                | FILE_DISPOSITION_FORCE_IMAGE_SECTION_CHECK.0,
        ),
    };

    let result = SetFileInformationByHandle(
        handle,
        FileDispositionInfoEx,
        &mut info as *mut _ as *mut _,
        std::mem::size_of::<FILE_DISPOSITION_INFORMATION_EX>() as u32,
    );

    CloseHandle(handle).ok();

    result.map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
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
                    Some(2) => {
                        // ERROR_FILE_NOT_FOUND - directory may be empty (ok to skip)
                        // This can happen with broken symlinks pointing to inaccessible paths
                        return Ok(());
                    }
                    Some(3) => {
                        // ERROR_PATH_NOT_FOUND - path is invalid/inaccessible
                        // For broken symlinks, this is expected; silently skip
                        // For normal directories, this indicates the path was deleted by another thread
                        return Ok(());
                    }
                    Some(5) => {
                        // ERROR_ACCESS_DENIED - permission issue, might be temporary
                        // Don't silently skip - this could lose files
                        return Err(err);
                    }
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
    /// Full path to the process executable (if available)
    pub exe_path: Option<String>,
}

/// Get the full executable path for a process by PID
#[cfg(windows)]
fn get_process_exe_path(pid: u32) -> Option<String> {
    use windows::Win32::System::Threading::QueryFullProcessImageNameW;
    use windows::Win32::System::Threading::PROCESS_NAME_FORMAT;

    // Skip system processes
    if pid == 0 || pid == 4 {
        return None;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut size = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        CloseHandle(handle).ok();
        result.ok()?;
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
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
            for info in proc_info.iter().take(proc_info_count as usize) {
                let pid = info.Process.dwProcessId;

                let name_len = info
                    .strAppName
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(info.strAppName.len());
                let name = String::from_utf16_lossy(&info.strAppName[..name_len]);

                let exe_path = get_process_exe_path(pid);
                processes.push(LockingProcess {
                    pid,
                    name,
                    exe_path,
                });
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
            for info in proc_info.iter().take(proc_info_count as usize) {
                let pid = info.Process.dwProcessId;
                let name_len = info
                    .strAppName
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(info.strAppName.len());
                let name = String::from_utf16_lossy(&info.strAppName[..name_len]);
                let exe_path = get_process_exe_path(pid);
                processes.push(LockingProcess {
                    pid,
                    name,
                    exe_path,
                });
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

        result.map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
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

pub fn is_file_in_use_error(error: &io::Error) -> bool {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    matches!(
        error.raw_os_error(),
        Some(ERROR_ACCESS_DENIED) | Some(ERROR_SHARING_VIOLATION) | Some(ERROR_LOCK_VIOLATION)
    )
}

pub fn is_dir_not_empty_error(error: &io::Error) -> bool {
    const ERROR_DIR_NOT_EMPTY: i32 = 145;
    error.raw_os_error() == Some(ERROR_DIR_NOT_EMPTY)
}

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

// ============================================================================
// NtQuerySystemInformation(SystemHandleInformation) + DuplicateHandle 强制解锁
//
// 枚举系统所有打开的句柄，找到指向目标文件的句柄，
// 用 DuplicateHandle(DUPLICATE_CLOSE_SOURCE) 在远程进程中强制关闭。
// 与火绒安全/Unlocker 相同的内核级句柄关闭机制。
// ============================================================================

/// Undocumented SystemHandleInformation class (0x10)
#[cfg(windows)]
const SYSTEM_HANDLE_INFORMATION_CLASS: SYSTEM_INFORMATION_CLASS = SYSTEM_INFORMATION_CLASS(0x10);

#[cfg(windows)]
#[repr(C)]
#[derive(Copy, Clone)]
struct SystemHandleInformation {
    number_of_handles: u32,
    handles: [SystemHandleTableEntryInfo; 1],
}

#[cfg(windows)]
#[repr(C)]
#[derive(Copy, Clone)]
struct SystemHandleTableEntryInfo {
    unique_process_id: u16,
    _creator_back_trace_index: u16,
    object_type_index: u8,
    _handle_attributes: u8,
    handle_value: u16,
    _object: usize,
    granted_access: u32,
}

/// Force-close all file handles pointing to the given paths.
///
/// Only releases locks — does NOT delete anything.
/// Uses NtQuerySystemInformation + DuplicateHandle(DUPLICATE_CLOSE_SOURCE).
///
/// # Safety concern
/// Closing handles in another process may crash that process.
/// Only call when user explicitly opted in (--kill-processes).
#[cfg(windows)]
pub fn force_close_file_handles(paths: &[PathBuf], verbose: bool) -> io::Result<usize> {
    if paths.is_empty() {
        return Ok(0);
    }

    let normalized_targets: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            let abs = std::fs::canonicalize(p).ok()?;
            Some(abs.to_string_lossy().to_lowercase())
        })
        .collect();

    if normalized_targets.is_empty() {
        return Ok(0);
    }

    let file_type_index = detect_file_object_type_index();

    let buf = query_system_handles()?;
    let info = buf.as_ptr() as *const SystemHandleInformation;
    let num_handles = unsafe { (*info).number_of_handles as usize };

    if verbose {
        eprintln!(
            "Scanning {} system handles for locked files...",
            num_handles
        );
    }

    let entries = unsafe { std::slice::from_raw_parts((*info).handles.as_ptr(), num_handles) };

    let current_pid = std::process::id() as u16;
    let mut handles_closed = 0usize;
    let mut proc_cache: std::collections::HashMap<u16, Option<HANDLE>> =
        std::collections::HashMap::new();
    let current_process = unsafe { GetCurrentProcess() };

    for entry in entries {
        let pid = entry.unique_process_id;
        if pid == current_pid || pid == 0 || pid == 4 || entry.granted_access == 0 {
            continue;
        }

        if let Some(file_idx) = file_type_index {
            if entry.object_type_index != file_idx {
                continue;
            }
        }

        let proc_handle = proc_cache
            .entry(pid)
            .or_insert_with(|| unsafe { OpenProcess(PROCESS_DUP_HANDLE, false, pid as u32).ok() });

        let proc_handle = match proc_handle {
            Some(h) => *h,
            None => continue,
        };

        let source_handle = HANDLE(entry.handle_value as *mut c_void);
        let mut dup_handle = HANDLE::default();

        if unsafe {
            DuplicateHandle(
                proc_handle,
                source_handle,
                current_process,
                &mut dup_handle,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )
        }
        .is_err()
        {
            continue;
        }

        let is_match = resolve_handle_path_with_timeout(dup_handle)
            .map(|p| normalized_targets.contains(&p.to_lowercase()))
            .unwrap_or(false);

        unsafe { CloseHandle(dup_handle).ok() };

        if is_match {
            let ok = unsafe {
                DuplicateHandle(
                    proc_handle,
                    source_handle,
                    HANDLE::default(),
                    std::ptr::null_mut(),
                    0,
                    false,
                    DUPLICATE_CLOSE_SOURCE,
                )
            }
            .is_ok();

            if ok {
                handles_closed += 1;
                if verbose {
                    eprintln!(
                        "  Closed handle 0x{:04X} in PID {}",
                        entry.handle_value, pid
                    );
                }
            }
        }
    }

    for (_, h) in proc_cache {
        if let Some(h) = h {
            unsafe { CloseHandle(h).ok() };
        }
    }

    if verbose && handles_closed > 0 {
        eprintln!("Force-closed {} handle(s)", handles_closed);
    }

    Ok(handles_closed)
}

const RESOLVE_TIMEOUT: Duration = Duration::from_millis(200);

#[cfg(windows)]
fn resolve_handle_path_with_timeout(handle: HANDLE) -> Option<String> {
    let handle_val = handle.0 as usize;
    let (tx, rx) = std::sync::mpsc::channel();

    thread::spawn(move || {
        let h = HANDLE(handle_val as *mut c_void);
        let mut buf = [0u16; 1024];
        let len = unsafe { GetFinalPathNameByHandleW(h, &mut buf, FILE_NAME_NORMALIZED) };
        if len > 0 && (len as usize) < buf.len() {
            let _ = tx.send(Some(String::from_utf16_lossy(&buf[..len as usize])));
        } else {
            let _ = tx.send(None);
        }
    });

    rx.recv_timeout(RESOLVE_TIMEOUT).ok().flatten()
}

/// 运行时检测 File 对象的 object_type_index（不同 Windows 版本值不同）。
/// 通过打开 NUL 设备获取一个已知的 File 句柄，然后在系统句柄表中找到它的 type index。
#[cfg(windows)]
fn detect_file_object_type_index() -> Option<u8> {
    let nul_path = path_to_wide(Path::new("NUL"));
    let nul_handle = unsafe {
        CreateFileW(
            PCWSTR(nul_path.as_ptr()),
            DELETE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            HANDLE::default(),
        )
    }
    .ok()?;

    let current_pid = std::process::id() as u16;
    let nul_handle_value = nul_handle.0 as u16;

    let buf = query_system_handles().ok()?;
    let info = buf.as_ptr() as *const SystemHandleInformation;
    let num_handles = unsafe { (*info).number_of_handles as usize };
    let entries = unsafe { std::slice::from_raw_parts((*info).handles.as_ptr(), num_handles) };

    let mut result = None;
    for entry in entries {
        if entry.unique_process_id == current_pid && entry.handle_value == nul_handle_value {
            result = Some(entry.object_type_index);
            break;
        }
    }

    unsafe { CloseHandle(nul_handle).ok() };
    result
}

#[cfg(windows)]
fn query_system_handles() -> io::Result<Vec<u8>> {
    let mut buf_size: usize = 4 * 1024 * 1024;
    let mut buf: Vec<u8> = vec![0u8; buf_size];

    for _ in 0..10 {
        let mut return_length: u32 = 0;
        let status: NTSTATUS = unsafe {
            NtQuerySystemInformation(
                SYSTEM_HANDLE_INFORMATION_CLASS,
                buf.as_mut_ptr() as *mut c_void,
                buf_size as u32,
                &mut return_length,
            )
        };

        if status == STATUS_INFO_LENGTH_MISMATCH {
            buf_size = (return_length as usize) * 3 / 2;
            buf.resize(buf_size, 0);
            continue;
        }

        if status.is_ok() {
            return Ok(buf);
        }

        return Err(io::Error::other(format!(
            "NtQuerySystemInformation failed: 0x{:08X}",
            status.0 as u32
        )));
    }

    Err(io::Error::other(
        "NtQuerySystemInformation: buffer resize limit",
    ))
}

#[cfg(not(windows))]
pub fn force_close_file_handles(_paths: &[PathBuf], _verbose: bool) -> io::Result<usize> {
    Ok(0)
}
