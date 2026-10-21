//! The language of the Explorer menu.
//!
//! The menu strings are compiled into the DLL, and the DLL is loaded into
//! Explorer, so the set is picked when the menu is built. The `Language` value
//! of the settings key decides; without it the Windows UI language decides, and
//! languages rmx does not ship get English.

use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Globalization::GetUserDefaultUILanguage;
use windows::Win32::System::Registry::*;

const SETTINGS_REG_KEY: &str = "Software\\rmx\\Settings";
const LANGUAGE_VALUE: &str = "Language";
/// Primary language id of Chinese, `LANG_CHINESE` from `winnt.h`.
const LANG_CHINESE: u16 = 0x04;

/// A language the menu can be shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    /// The language of the menu: the settings key, then the Windows UI language.
    pub fn current() -> Lang {
        read_setting()
            .and_then(|code| Lang::from_code(&code))
            .unwrap_or_else(system_language)
    }

    /// Parses a language code (`zh`, `en`), as used by the `Language` value of
    /// the settings key.
    pub fn from_code(code: &str) -> Option<Lang> {
        match code.trim().to_ascii_lowercase().as_str() {
            "zh" => Some(Lang::Zh),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    /// The language to use for a Windows language id. Languages rmx does not
    /// ship get English.
    pub fn from_lcid(lcid: u16) -> Lang {
        if lcid & 0x3ff == LANG_CHINESE {
            Lang::Zh
        } else {
            Lang::En
        }
    }
}

fn system_language() -> Lang {
    Lang::from_lcid(unsafe { GetUserDefaultUILanguage() })
}

/// The `Language` value of the settings key, when it can be read.
fn read_setting() -> Option<String> {
    let key_wide: Vec<u16> = SETTINGS_REG_KEY
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let value_wide: Vec<u16> = LANGUAGE_VALUE
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_wide.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        );
        if result != ERROR_SUCCESS {
            return None;
        }

        let mut buffer = [0u16; 32];
        let mut data_size = (buffer.len() * std::mem::size_of::<u16>()) as u32;
        let mut value_type = REG_NONE;
        let result = RegQueryValueExW(
            hkey,
            PCWSTR(value_wide.as_ptr()),
            None,
            Some(&mut value_type),
            Some(buffer.as_mut_ptr() as *mut u8),
            Some(&mut data_size),
        );
        let _ = RegCloseKey(hkey);

        if result != ERROR_SUCCESS || value_type != REG_SZ {
            return None;
        }

        let length = (data_size as usize / std::mem::size_of::<u16>()).min(buffer.len());
        let code = String::from_utf16_lossy(&buffer[..length]);
        Some(code.trim_end_matches('\0').to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_are_known_by_name() {
        assert_eq!(Lang::from_code("zh"), Some(Lang::Zh));
        assert_eq!(Lang::from_code("EN"), Some(Lang::En));
        assert_eq!(Lang::from_code(" de "), None);
    }

    #[test]
    fn chinese_language_ids_get_chinese() {
        assert_eq!(Lang::from_lcid(0x0804), Lang::Zh); // zh-CN
        assert_eq!(Lang::from_lcid(0x0409), Lang::En); // en-US
        assert_eq!(Lang::from_lcid(0x0419), Lang::En); // ru-RU
    }
}
