use std::env;
use std::io::{self, ErrorKind};
use win_ctx::{ActivationType, CtxEntry, EntryOptions, MenuPosition};

const ENTRY_NAME: &str = "Delete with rmx";

pub fn install() -> io::Result<()> {
    let exe_path = get_exe_path()?;

    if is_installed()? {
        return Err(io::Error::new(
            ErrorKind::AlreadyExists,
            "rmx is already installed in the context menu. Use 'rmx uninstall' first to reinstall.",
        ));
    }

    let folder_command = format!("\"{}\" -rf --gui \"%V\"", exe_path);
    let file_command = format!("\"{}\" -f --gui \"%V\"", exe_path);

    CtxEntry::new_with_options(
        ENTRY_NAME,
        &ActivationType::Folder,
        &EntryOptions {
            command: Some(folder_command),
            icon: None,
            position: Some(MenuPosition::Bottom),
            separator: None,
            extended: false,
        },
    )?;

    CtxEntry::new_with_options(
        ENTRY_NAME,
        &ActivationType::File("*".to_string()),
        &EntryOptions {
            command: Some(file_command),
            icon: None,
            position: Some(MenuPosition::Bottom),
            separator: None,
            extended: false,
        },
    )?;

    Ok(())
}

pub fn uninstall() -> io::Result<()> {
    if !is_installed()? {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            "rmx is not installed in the context menu.",
        ));
    }

    if let Some(entry) = CtxEntry::get(&[ENTRY_NAME], &ActivationType::Folder) {
        entry.delete()?;
    }

    if let Some(entry) = CtxEntry::get(&[ENTRY_NAME], &ActivationType::File("*".to_string())) {
        entry.delete()?;
    }

    Ok(())
}

pub fn is_installed() -> io::Result<bool> {
    let folder_installed = CtxEntry::get(&[ENTRY_NAME], &ActivationType::Folder).is_some();
    let file_installed =
        CtxEntry::get(&[ENTRY_NAME], &ActivationType::File("*".to_string())).is_some();
    Ok(folder_installed || file_installed)
}

fn get_exe_path() -> io::Result<String> {
    env::current_exe()?
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                "Executable path contains invalid Unicode characters",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_exe_path() {
        let path = get_exe_path();
        assert!(path.is_ok());
        let path = path.unwrap();
        assert!(path.ends_with(".exe"));
    }
}
