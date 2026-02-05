use std::env;
use std::path::{Path, PathBuf};

pub fn is_system_directory(path: &Path) -> bool {
    let canonical = path.canonicalize().ok();
    let path_str = path.to_string_lossy();
    let canonical_str = canonical.as_ref().map(|p| p.to_string_lossy());

    #[cfg(windows)]
    {
        let protected_windows = [
            "C:\\Windows",
            "C:\\Windows\\System32",
            "C:\\Program Files",
            "C:\\Program Files (x86)",
            "C:\\ProgramData",
            "C:\\",
            "C:\\Users",
        ];

        for protected in &protected_windows {
            if path_str.eq_ignore_ascii_case(protected) {
                return true;
            }
            if let Some(ref canonical) = canonical_str {
                if canonical.eq_ignore_ascii_case(protected) {
                    return true;
                }
            }
        }

        if path_str.len() <= 3 && path_str.ends_with(":\\") {
            return true;
        }
    }

    #[cfg(unix)]
    {
        let protected_unix = [
            "/", "/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/root", "/sbin",
            "/sys", "/usr", "/var",
        ];

        for protected in &protected_unix {
            if path_str == *protected {
                return true;
            }
            if let Some(ref canonical) = canonical_str {
                if canonical.as_ref() == *protected {
                    return true;
                }
            }
        }
    }

    if let Ok(home) = env::var("HOME") {
        let home_path = PathBuf::from(home);
        if let (Ok(p1), Ok(p2)) = (path.canonicalize(), home_path.canonicalize()) {
            if p1 == p2 {
                return true;
            }
        }
    }

    #[cfg(windows)]
    {
        if let Ok(userprofile) = env::var("USERPROFILE") {
            let user_path = PathBuf::from(userprofile);
            if let (Ok(p1), Ok(p2)) = (path.canonicalize(), user_path.canonicalize()) {
                if p1 == p2 {
                    return true;
                }
            }
        }
    }

    false
}

pub fn is_in_current_directory(path: &Path) -> bool {
    if let Ok(cwd) = env::current_dir() {
        if let (Ok(p1), Ok(p2)) = (path.canonicalize(), cwd.canonicalize()) {
            return p1 == p2 || cwd.starts_with(&p1);
        }
    }
    false
}

fn get_danger_reason(path: &Path) -> Option<String> {
    if is_system_directory(path) {
        return Some(format!(
            "'{}' is a system directory - deleting it could break your system",
            path.display()
        ));
    }

    if is_in_current_directory(path) {
        return Some(format!(
            "'{}' contains or is your current working directory",
            path.display()
        ));
    }

    None
}

#[derive(Debug)]
pub enum SafetyCheck {
    Safe,
    Dangerous { reason: String, can_override: bool },
}

pub fn check_path_safety(path: &Path) -> SafetyCheck {
    if let Some(reason) = get_danger_reason(path) {
        SafetyCheck::Dangerous {
            reason,
            can_override: !is_system_directory(path),
        }
    } else {
        SafetyCheck::Safe
    }
}
