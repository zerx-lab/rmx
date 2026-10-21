//! Interface strings for the languages rmx ships with.
//!
//! The strings are compiled into the binary: the exe is copied to other places
//! and the shell DLL is loaded into Explorer, so a file next to the binary
//! cannot be read. Each language has its own module with a [`Strings`] table,
//! so a missing translation is a compile error.

mod en;
mod zh;

/// A language the interface can be shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    /// Chinese, the language of the upstream interface.
    Zh,
    /// English, the language of the command line interface.
    #[default]
    En,
}

/// Primary language id of Chinese, `LANG_CHINESE` from `winnt.h`.
const LANG_CHINESE: u16 = 0x04;

impl Lang {
    /// Parses a language code (`zh`, `en`), as used by `--lang` and by the
    /// `Language` value of the settings key.
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

/// Every string the windows show.
///
/// A language fills every field of this table, so a translation cannot be
/// forgotten: the compiler reports each field that is missing.
///
/// A `_one` and a `_many` field form a pair that [`plural`] chooses between:
/// the `_one` text is complete, the `_many` text is a template.
pub struct Strings {
    /// Progress window: state title while the deletion runs.
    pub deleting: &'static str,
    /// Progress window: state title after a deletion without errors.
    pub delete_complete: &'static str,
    /// Progress window: state title after a deletion with errors.
    pub delete_complete_errors: &'static str,
    /// Progress window: status text for exactly one error.
    pub done_error_one: &'static str,
    /// Progress window: status text for several errors, holds the count.
    pub done_errors_many: &'static str,
    /// Progress window: status text when nothing failed.
    pub completed: &'static str,
    /// Progress window: deleted and total directories, holds both counts. The
    /// total selects the form.
    pub deleted_dirs_one: &'static str,
    /// Progress window: deleted and total directories, holds both counts.
    pub deleted_dirs_many: &'static str,
    /// Progress window: button that copies the errors to the clipboard.
    pub copy_errors: &'static str,
    /// Button that closes a window.
    pub close: &'static str,
    /// Button that closes a window without the action.
    pub cancel: &'static str,

    /// Confirmation dialog: title of the dialog and of its window.
    pub confirm_deletion: &'static str,
    /// Confirmation dialog: note under the title.
    pub confirm_warning: &'static str,
    /// Confirmation dialog: path count of a multi-path selection.
    pub items_selected: &'static str,
    /// Confirmation dialog: the file count of one file. It is also the text of
    /// the single-file case, so it holds the number.
    pub files_one: &'static str,
    /// Confirmation dialog: the file count of several files.
    pub files_many: &'static str,
    /// Confirmation dialog: the directory count of one directory.
    pub dirs_one: &'static str,
    /// Confirmation dialog: the directory count of several directories.
    pub dirs_many: &'static str,
    /// Confirmation dialog: separator between the file and directory counts.
    pub item_separator: &'static str,
    /// Confirmation dialog: checkbox that skips the next confirmations.
    pub dont_ask_again: &'static str,
    /// Confirmation dialog: button that starts the deletion.
    pub delete: &'static str,

    /// Unlock window: title of the window and of the header.
    pub unlock_files: &'static str,
    /// Unlock window: subtitle that explains the action.
    pub unlock_help: &'static str,
    /// Unlock window: header while the processes are terminated.
    pub unlocking: &'static str,
    /// Unlock window: subtitle while the processes are terminated.
    pub terminating_locking: &'static str,
    /// Unlock window: result title when every process was terminated.
    pub unlock_successful: &'static str,
    /// Unlock window: result text with one terminated process.
    pub terminated_one: &'static str,
    /// Unlock window: result text with several terminated processes.
    pub terminated_many: &'static str,
    /// Unlock window: result title when some processes survived.
    pub some_unlocks_failed: &'static str,
    /// Unlock window: result text with the numbers of successes and failures.
    pub succeeded_failed: &'static str,
    /// Unlock window: text before the list of one file or folder.
    pub unlock_following_one: &'static str,
    /// Unlock window: text before the list of several files or folders.
    pub unlock_following_many: &'static str,
    /// Unlock window: column with the names of the files to unlock.
    pub file_folder_name: &'static str,
    /// Unlock window: column with the names of the locking processes.
    pub name: &'static str,
    /// Unlock window: column with the paths of the locking processes.
    pub path: &'static str,
    /// Unlock window: column with the state of a file to unlock.
    pub status: &'static str,
    /// Unlock window: file state while the processes are terminated.
    pub status_unlocking: &'static str,
    /// Unlock window: file state before the processes are terminated.
    pub status_pending: &'static str,
    /// Unlock window: heading of the table of locking processes, holds a name.
    pub locked_by: &'static str,
    /// Unlock window: column with the process that kept a file locked.
    pub process: &'static str,
    /// Unlock window: column with the reason why a process survived.
    pub reason: &'static str,
    /// Unlock window: button that terminates the locking processes.
    pub unlock_all: &'static str,
    /// Unlock window: note shown while the processes are terminated.
    pub processing: &'static str,
    /// Button that closes a window after the result.
    pub ok: &'static str,

    /// Window without locks: text of the window.
    pub no_locked_files: &'static str,

    /// Title of the progress window.
    pub deletion_progress: &'static str,
}

/// The strings of `lang`.
pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::Zh => &zh::STRINGS,
        Lang::En => &en::STRINGS,
    }
}

/// Fills the `{}` placeholders of `template` with `values`, in order.
///
/// The tables hold format templates, but `format!` needs its template at
/// compile time, so the values are inserted here instead.
pub fn fill(template: &str, values: &[&dyn std::fmt::Display]) -> String {
    let mut filled = String::with_capacity(template.len() + 16);
    let mut rest = template;

    for value in values {
        match rest.split_once("{}") {
            Some((head, tail)) => {
                filled.push_str(head);
                filled.push_str(&value.to_string());
                rest = tail;
            }
            None => break,
        }
    }

    filled.push_str(rest);
    filled
}

/// The text that fits `value`: the singular text for one item, the plural
/// template otherwise.
///
/// A language without plural forms puts the same text in both fields.
pub fn plural<'a>(one: &'a str, many: &'a str, value: usize) -> &'a str {
    if value == 1 {
        one
    } else {
        many
    }
}

/// [`fill`] with the text that fits `value` and the value itself.
pub fn count(one: &str, many: &str, value: usize) -> String {
    fill(plural(one, many, value), &[&value])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_inserts_values_in_order() {
        assert_eq!(fill("{} of {}", &[&3, &7]), "3 of 7");
        assert_eq!(fill("no placeholders", &[&1]), "no placeholders");
        assert_eq!(fill("{} of {}", &[&3]), "3 of {}");
    }

    #[test]
    fn count_picks_the_text_that_fits_the_value() {
        assert_eq!(count("1 file", "{} files", 1), "1 file");
        assert_eq!(count("1 file", "{} files", 3), "3 files");
        assert_eq!(count("{} 个文件", "{} 个文件", 1), "1 个文件");
        assert_eq!(count("{} 个文件", "{} 个文件", 3), "3 个文件");
    }

    #[test]
    fn language_codes_are_known_by_name() {
        assert_eq!(Lang::from_code("zh"), Some(Lang::Zh));
        assert_eq!(Lang::from_code("EN"), Some(Lang::En));
        assert_eq!(Lang::from_code("de"), None);
    }

    #[test]
    fn chinese_language_ids_get_chinese() {
        assert_eq!(Lang::from_lcid(0x0804), Lang::Zh); // zh-CN
        assert_eq!(Lang::from_lcid(0x0409), Lang::En); // en-US
        assert_eq!(Lang::from_lcid(0x0419), Lang::En); // ru-RU
    }

    #[test]
    fn every_language_fills_the_tables() {
        for lang in [Lang::Zh, Lang::En] {
            let strings = strings(lang);
            assert!(!strings.delete_complete.is_empty());
            assert!(strings.done_errors_many.contains("{}"));
            assert!(!strings.files_one.is_empty());
        }
    }
}
