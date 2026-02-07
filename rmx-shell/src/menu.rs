use std::cell::RefCell;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::System::Registry::HKEY;
use windows::Win32::System::Threading::{
    CreateProcessW, DETACHED_PROCESS, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const MENU_TEXT: PCWSTR = w!("Delete with rmx");
const VERB: &str = "rmxdelete";

#[implement(IShellExtInit, IContextMenu)]
pub struct RmxContextMenu {
    selected_paths: RefCell<Vec<PathBuf>>,
}

impl RmxContextMenu {
    pub fn new() -> Self {
        crate::increment_object_count();
        Self {
            selected_paths: RefCell::new(Vec::new()),
        }
    }
}

impl Drop for RmxContextMenu {
    fn drop(&mut self) {
        crate::decrement_object_count();
    }
}

impl IShellExtInit_Impl for RmxContextMenu_Impl {
    fn Initialize(
        &self,
        _pidlfolder: *const ITEMIDLIST,
        pdtobj: Option<&IDataObject>,
        _hkeyprogid: HKEY,
    ) -> Result<()> {
        unsafe {
            let data_obj = pdtobj.ok_or(E_INVALIDARG)?;

            let format = FORMATETC {
                cfFormat: CF_HDROP.0,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            };

            let medium = data_obj.GetData(&format)?;
            let hdrop = HDROP(medium.u.hGlobal.0 as _);

            let file_count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
            let mut paths = Vec::with_capacity(file_count as usize);

            for i in 0..file_count {
                let char_count = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; char_count + 1];
                DragQueryFileW(hdrop, i, Some(&mut buf));
                let path = OsString::from_wide(&buf[..char_count]);
                paths.push(PathBuf::from(path));
            }

            *self.selected_paths.borrow_mut() = paths;
        }
        Ok(())
    }
}

impl IContextMenu_Impl for RmxContextMenu_Impl {
    fn QueryContextMenu(
        &self,
        hmenu: HMENU,
        indexmenu: u32,
        idcmdfirst: u32,
        _idcmdlast: u32,
        _uflags: u32,
    ) -> windows::core::Result<()> {
        unsafe {
            InsertMenuW(
                hmenu,
                indexmenu,
                MF_STRING | MF_BYPOSITION,
                idcmdfirst as usize,
                MENU_TEXT,
            )?;
        }

        // QueryContextMenu must return MAKE_HRESULT(SEVERITY_SUCCESS, 0, id_offset + 1).
        // Result<()> → HRESULT always maps Ok(()) to S_OK(0), losing the count.
        // Err path preserves the raw HRESULT code via Error::code().
        Err(Error::from(HRESULT(1)))
    }

    fn InvokeCommand(&self, pici: *const CMINVOKECOMMANDINFO) -> windows::core::Result<()> {
        let info = unsafe { &*pici };

        let is_verb = (info.lpVerb.0 as usize) > 0xFFFF;
        if is_verb {
            let verb_ptr = info.lpVerb.0 as *const u8;
            let verb = unsafe {
                let len = (0..).find(|&i| *verb_ptr.add(i) == 0).unwrap_or(0);
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(verb_ptr, len))
            };
            if verb != VERB {
                return Err(E_INVALIDARG.into());
            }
        }

        let paths = self.selected_paths.borrow();
        if paths.is_empty() {
            return Err(E_FAIL.into());
        }

        let exe_path = get_rmx_exe_path()?;

        for path in paths.iter() {
            let path_str = path.to_string_lossy();
            let flag = if path.is_dir() { "-r" } else { "" };
            let cmdline = if flag.is_empty() {
                format!("\"{}\" --gui --kill-processes \"{}\"", exe_path, path_str)
            } else {
                format!("\"{}\" {} --gui --kill-processes \"{}\"", exe_path, flag, path_str)
            };

            let mut cmdline_wide: Vec<u16> =
                cmdline.encode_utf16().chain(std::iter::once(0)).collect();

            unsafe {
                let mut si: STARTUPINFOW = std::mem::zeroed();
                si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
                let mut pi: PROCESS_INFORMATION = std::mem::zeroed();

                let _ = CreateProcessW(
                    PCWSTR::null(),
                    PWSTR(cmdline_wide.as_mut_ptr()),
                    None,
                    None,
                    false,
                    DETACHED_PROCESS,
                    None,
                    PCWSTR::null(),
                    &si,
                    &mut pi,
                );

                if !pi.hProcess.is_invalid() {
                    let _ = windows::Win32::Foundation::CloseHandle(pi.hProcess);
                }
                if !pi.hThread.is_invalid() {
                    let _ = windows::Win32::Foundation::CloseHandle(pi.hThread);
                }
            }
        }

        Ok(())
    }

    fn GetCommandString(
        &self,
        _idcmd: usize,
        utype: u32,
        _preserved: *const u32,
        pszname: PSTR,
        cchmax: u32,
    ) -> windows::core::Result<()> {
        const GCS_VERBA: u32 = 0;
        const GCS_VERBW: u32 = 4;

        match utype {
            GCS_VERBA => {
                let verb_bytes = VERB.as_bytes();
                let copy_len = verb_bytes.len().min(cchmax as usize - 1);
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        verb_bytes.as_ptr(),
                        pszname.0 as *mut u8,
                        copy_len,
                    );
                    *pszname.0.add(copy_len) = 0;
                }
            }
            GCS_VERBW => {
                let verb_wide: Vec<u16> = VERB.encode_utf16().chain(std::iter::once(0)).collect();
                let copy_len = verb_wide.len().min(cchmax as usize);
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        verb_wide.as_ptr(),
                        pszname.0 as *mut u16,
                        copy_len,
                    );
                }
            }
            _ => {}
        }

        Ok(())
    }
}

fn get_rmx_exe_path() -> Result<String> {
    // 1. Search PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(';') {
            let candidate = PathBuf::from(dir).join("rmx.exe");
            if candidate.is_file() {
                return Ok(candidate.to_string_lossy().into_owned());
            }
        }
    }

    // 2. Fallback: DLL sibling directory
    unsafe {
        let mut hmodule = HMODULE::default();
        windows::Win32::System::LibraryLoader::GetModuleHandleExW(
            windows::Win32::System::LibraryLoader::GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                | windows::Win32::System::LibraryLoader::GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(get_rmx_exe_path as *const u16),
            &mut hmodule,
        )?;

        let mut buffer = vec![0u16; 1024];
        let len = windows::Win32::System::LibraryLoader::GetModuleFileNameW(hmodule, &mut buffer);
        if len == 0 {
            return Err(E_FAIL.into());
        }

        let dll_dir = PathBuf::from(String::from_utf16_lossy(&buffer[..len as usize]));
        if let Some(parent) = dll_dir.parent() {
            let candidate = parent.join("rmx.exe");
            if candidate.is_file() {
                return Ok(candidate.to_string_lossy().into_owned());
            }
        }
    }

    Err(E_FAIL.into())
}
