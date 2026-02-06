use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Shell::*;

use crate::CLSID_RMX_CONTEXT_MENU;

const EXTENSION_NAME: &str = "RmxContextMenu";

fn get_dll_path() -> Result<String> {
    unsafe {
        // Use GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS to reliably get our own DLL's HMODULE,
        // regardless of whether DllMain was called.
        let mut hmodule = HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(get_dll_path as *const u16),
            &mut hmodule,
        )?;

        let mut buffer = vec![0u16; 1024];
        let len = GetModuleFileNameW(hmodule, &mut buffer);
        if len == 0 {
            return Err(Error::from_win32());
        }

        Ok(String::from_utf16_lossy(&buffer[..len as usize]))
    }
}

fn guid_to_string(guid: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

fn check_win32_error(err: WIN32_ERROR) -> Result<()> {
    if err == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(Error::from_win32())
    }
}

pub fn register_server() -> Result<()> {
    let dll_path = get_dll_path()?;
    let clsid_str = guid_to_string(&CLSID_RMX_CONTEXT_MENU);

    unsafe {
        let clsid_key = format!("Software\\Classes\\CLSID\\{}", clsid_str);
        let clsid_key_wide: Vec<u16> = clsid_key.encode_utf16().chain(std::iter::once(0)).collect();

        let mut hkey = HKEY::default();
        check_win32_error(RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(clsid_key_wide.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        ))?;

        let name_wide: Vec<u16> = "rmx Context Menu"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        check_win32_error(RegSetValueExW(
            hkey,
            PCWSTR::null(),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                name_wide.as_ptr() as *const u8,
                name_wide.len() * 2,
            )),
        ))?;
        let _ = RegCloseKey(hkey);

        let inproc_key = format!("{}\\InprocServer32", clsid_key);
        let inproc_key_wide: Vec<u16> = inproc_key
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        check_win32_error(RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(inproc_key_wide.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        ))?;

        let dll_path_wide: Vec<u16> = dll_path.encode_utf16().chain(std::iter::once(0)).collect();
        check_win32_error(RegSetValueExW(
            hkey,
            PCWSTR::null(),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                dll_path_wide.as_ptr() as *const u8,
                dll_path_wide.len() * 2,
            )),
        ))?;

        let threading_model: Vec<u16> = "Apartment"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let threading_model_name: Vec<u16> = "ThreadingModel"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        check_win32_error(RegSetValueExW(
            hkey,
            PCWSTR(threading_model_name.as_ptr()),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                threading_model.as_ptr() as *const u8,
                threading_model.len() * 2,
            )),
        ))?;
        let _ = RegCloseKey(hkey);

        let dir_handler_key = format!(
            "Software\\Classes\\Directory\\shellex\\ContextMenuHandlers\\{}",
            EXTENSION_NAME
        );
        let dir_handler_key_wide: Vec<u16> = dir_handler_key
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        check_win32_error(RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(dir_handler_key_wide.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        ))?;

        let clsid_value: Vec<u16> = clsid_str.encode_utf16().chain(std::iter::once(0)).collect();
        check_win32_error(RegSetValueExW(
            hkey,
            PCWSTR::null(),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                clsid_value.as_ptr() as *const u8,
                clsid_value.len() * 2,
            )),
        ))?;
        let _ = RegCloseKey(hkey);

        let file_handler_key = format!(
            "Software\\Classes\\*\\shellex\\ContextMenuHandlers\\{}",
            EXTENSION_NAME
        );
        let file_handler_key_wide: Vec<u16> = file_handler_key
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        check_win32_error(RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(file_handler_key_wide.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        ))?;

        check_win32_error(RegSetValueExW(
            hkey,
            PCWSTR::null(),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                clsid_value.as_ptr() as *const u8,
                clsid_value.len() * 2,
            )),
        ))?;
        let _ = RegCloseKey(hkey);

        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }

    Ok(())
}

pub fn unregister_server() -> Result<()> {
    let clsid_str = guid_to_string(&CLSID_RMX_CONTEXT_MENU);

    unsafe {
        let dir_handler_key = format!(
            "Software\\Classes\\Directory\\shellex\\ContextMenuHandlers\\{}",
            EXTENSION_NAME
        );
        let dir_handler_key_wide: Vec<u16> = dir_handler_key
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(dir_handler_key_wide.as_ptr()));

        let file_handler_key = format!(
            "Software\\Classes\\*\\shellex\\ContextMenuHandlers\\{}",
            EXTENSION_NAME
        );
        let file_handler_key_wide: Vec<u16> = file_handler_key
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(file_handler_key_wide.as_ptr()));

        let inproc_key = format!("Software\\Classes\\CLSID\\{}\\InprocServer32", clsid_str);
        let inproc_key_wide: Vec<u16> = inproc_key
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(inproc_key_wide.as_ptr()));

        let clsid_key = format!("Software\\Classes\\CLSID\\{}", clsid_str);
        let clsid_key_wide: Vec<u16> = clsid_key.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(clsid_key_wide.as_ptr()));

        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }

    Ok(())
}
