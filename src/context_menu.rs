use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;

use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Shell::*;

use crate::winapi;

/// rmx-shell.dll 编译时嵌入的字节
const SHELL_DLL_BYTES: &[u8] = include_bytes!(env!("RMX_SHELL_DLL_PATH"));

const CLSID_STR: &str = "{8A5B2C4D-6E7F-4A8B-9C0D-1E2F3A4B5C6D}";
const EXTENSION_NAME: &str = "RmxContextMenu";

/// Initialize rmx shell extension.
///
/// - 如果已安装，先卸载再重新安装
/// - 如果未安装，直接安装注册
///
/// 步骤:
/// 1. 清理旧版 win_ctx 注册的右键菜单项（如果有）
/// 2. 卸载已有的 shell extension（如果有）
/// 3. 释放 rmx-shell.dll 到 rmx.exe 同级目录
/// 4. 注册 COM shell extension
pub fn init() -> io::Result<()> {
    cleanup_legacy_entries();

    if is_shell_installed() {
        unregister_shell()?;
    }

    let dll_path = deploy_shell_dll()?;
    register_shell(&dll_path)?;

    Ok(())
}

/// 检查 shell extension 是否已注册
fn is_shell_installed() -> bool {
    let clsid_key = format!("Software\\Classes\\CLSID\\{}", CLSID_STR);
    let clsid_key_wide: Vec<u16> = clsid_key.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(clsid_key_wide.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        );
        if result == ERROR_SUCCESS {
            let _ = RegCloseKey(hkey);
            true
        } else {
            false
        }
    }
}

/// 释放嵌入的 rmx-shell.dll 到 rmx.exe 同级目录
///
/// 如果 DLL 被 Explorer 占用（已加载的 COM shell extension），
/// 会强制关闭文件句柄后重试写入。
fn deploy_shell_dll() -> io::Result<PathBuf> {
    let dll_path = get_shell_dll_path()?;

    match std::fs::write(&dll_path, SHELL_DLL_BYTES) {
        Ok(()) => return Ok(dll_path),
        Err(e) if is_file_locked_error(&e) => {
            retry_with_explorer_restart(&dll_path, || std::fs::write(&dll_path, SHELL_DLL_BYTES))?;
        }
        Err(e) => return Err(e),
    }

    Ok(dll_path)
}

/// 注册 shell extension COM 对象和右键菜单处理程序
fn register_shell(dll_path: &std::path::Path) -> io::Result<()> {
    let dll_path_str = dll_path.to_str().ok_or_else(|| {
        io::Error::new(ErrorKind::InvalidData, "DLL path contains invalid Unicode")
    })?;

    unsafe {
        // 1. 注册 CLSID
        let clsid_key = format!("Software\\Classes\\CLSID\\{}", CLSID_STR);
        let hkey = create_reg_key(&clsid_key)?;
        set_reg_value(hkey, None, "rmx Context Menu")?;
        let _ = RegCloseKey(hkey);

        // 2. 注册 InprocServer32
        let inproc_key = format!("{}\\InprocServer32", clsid_key);
        let hkey = create_reg_key(&inproc_key)?;
        set_reg_value(hkey, None, dll_path_str)?;
        set_reg_value(hkey, Some("ThreadingModel"), "Apartment")?;
        let _ = RegCloseKey(hkey);

        // 3. 注册 Directory context menu handler
        let dir_handler_key = format!(
            "Software\\Classes\\Directory\\shellex\\ContextMenuHandlers\\{}",
            EXTENSION_NAME
        );
        let hkey = create_reg_key(&dir_handler_key)?;
        set_reg_value(hkey, None, CLSID_STR)?;
        let _ = RegCloseKey(hkey);

        // 4. 注册 File context menu handler
        let file_handler_key = format!(
            "Software\\Classes\\*\\shellex\\ContextMenuHandlers\\{}",
            EXTENSION_NAME
        );
        let hkey = create_reg_key(&file_handler_key)?;
        set_reg_value(hkey, None, CLSID_STR)?;
        let _ = RegCloseKey(hkey);

        // 通知 Explorer 刷新
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }

    Ok(())
}

pub fn uninstall() -> io::Result<()> {
    cleanup_legacy_entries();
    unregister_shell()?;

    let dll_path = get_shell_dll_path()?;
    if dll_path.exists() {
        std::thread::sleep(std::time::Duration::from_millis(200));

        if let Err(e) = std::fs::remove_file(&dll_path) {
            if is_file_locked_error(&e) {
                retry_with_explorer_restart(&dll_path, || std::fs::remove_file(&dll_path))?;
            } else {
                return Err(e);
            }
        }
    }

    Ok(())
}

fn unregister_shell() -> io::Result<()> {
    unsafe {
        delete_reg_tree(&format!(
            "Software\\Classes\\Directory\\shellex\\ContextMenuHandlers\\{}",
            EXTENSION_NAME
        ));
        delete_reg_tree(&format!(
            "Software\\Classes\\*\\shellex\\ContextMenuHandlers\\{}",
            EXTENSION_NAME
        ));
        delete_reg_tree(&format!(
            "Software\\Classes\\CLSID\\{}\\InprocServer32",
            CLSID_STR
        ));
        delete_reg_tree(&format!("Software\\Classes\\CLSID\\{}", CLSID_STR));

        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }

    Ok(())
}

/// 清理旧版 win_ctx 方式注册的右键菜单项
fn cleanup_legacy_entries() {
    // win_ctx 在这些位置注册 "Delete with rmx" 项
    delete_reg_tree("Software\\Classes\\Directory\\shell\\Delete with rmx");
    delete_reg_tree("Software\\Classes\\*\\shell\\Delete with rmx");
}

fn get_shell_dll_path() -> io::Result<PathBuf> {
    let exe_dir = std::env::current_exe()?
        .parent()
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "Cannot determine exe directory"))?
        .to_path_buf();
    Ok(exe_dir.join("rmx-shell.dll"))
}

fn locking_processes_hint(path: &PathBuf) -> String {
    match winapi::find_locking_processes(path) {
        Ok(procs) if !procs.is_empty() => {
            let list: Vec<String> = procs
                .iter()
                .map(|p| format!("{} (PID {})", p.name, p.pid))
                .collect();
            format!("\n占用进程: {}", list.join(", "))
        }
        _ => String::new(),
    }
}

/// ERROR_ACCESS_DENIED (5) 或 ERROR_SHARING_VIOLATION (32)
/// Explorer 加载 COM DLL 时，删除文件可能返回其中任一错误码
fn is_file_locked_error(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5) | Some(32))
}

// ── Explorer restart ─────────────────────────────────────────────────────

/// DLL 被占用时的重试策略:
/// 1. force_close_file_handles → 重试
/// 2. 仍失败则杀 explorer.exe → 重试 → 重启 explorer.exe
fn retry_with_explorer_restart<F>(dll_path: &Path, op: F) -> io::Result<()>
where
    F: Fn() -> io::Result<()>,
{
    let _ = winapi::force_close_file_handles(&[dll_path.to_path_buf()], false);
    std::thread::sleep(std::time::Duration::from_millis(100));

    match op() {
        Ok(()) => return Ok(()),
        Err(e) if !is_file_locked_error(&e) => return Err(e),
        Err(_) => {}
    }

    eprintln!(
        "DLL 被占用，正在重启 Explorer...{}",
        locking_processes_hint(&dll_path.to_path_buf())
    );

    kill_explorer();
    std::thread::sleep(std::time::Duration::from_millis(500));

    let result = op();

    start_explorer();

    result
}

fn kill_explorer() {
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "explorer.exe"])
        .output();
}

fn start_explorer() {
    let _ = Command::new("cmd")
        .args(["/C", "start", "explorer.exe"])
        .output();
}

// ── Registry helpers ──────────────────────────────────────────────────────

unsafe fn create_reg_key(subkey: &str) -> io::Result<HKEY> {
    let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hkey = HKEY::default();

    let result = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        PCWSTR(subkey_wide.as_ptr()),
        0,
        PCWSTR::null(),
        REG_OPTION_NON_VOLATILE,
        KEY_WRITE,
        None,
        &mut hkey,
        None,
    );

    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }

    Ok(hkey)
}

unsafe fn set_reg_value(hkey: HKEY, name: Option<&str>, value: &str) -> io::Result<()> {
    let name_wide: Option<Vec<u16>> =
        name.map(|n| n.encode_utf16().chain(std::iter::once(0)).collect());
    let value_wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();

    let name_ptr = match &name_wide {
        Some(v) => PCWSTR(v.as_ptr()),
        None => PCWSTR::null(),
    };

    let result = RegSetValueExW(
        hkey,
        name_ptr,
        0,
        REG_SZ,
        Some(std::slice::from_raw_parts(
            value_wide.as_ptr() as *const u8,
            value_wide.len() * 2,
        )),
    );

    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }

    Ok(())
}

fn delete_reg_tree(subkey: &str) {
    let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(subkey_wide.as_ptr()));
    }
}
