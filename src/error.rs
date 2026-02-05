use std::fmt;
use std::io;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io {
        path: Option<PathBuf>,
        source: io::Error,
    },
    InvalidPath {
        path: PathBuf,
        reason: String,
    },
    PartialFailure {
        total: usize,
        failed: usize,
        errors: Vec<FailedItem>,
    },
}

#[derive(Debug, Clone)]
pub struct FailedItem {
    pub path: PathBuf,
    pub error: String,
    pub is_dir: bool,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => {
                if let Some(p) = path {
                    write!(f, "I/O error for '{}': {}", p.display(), source)
                } else {
                    write!(f, "I/O error: {}", source)
                }
            }
            Error::InvalidPath { path, reason } => {
                write!(f, "Invalid path '{}': {}", path.display(), reason)
            }
            Error::PartialFailure { total, failed, .. } => {
                write!(
                    f,
                    "Partial deletion failure: {}/{} items failed",
                    failed, total
                )
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io {
            path: None,
            source: err,
        }
    }
}

impl Error {
    pub fn io_with_path(path: PathBuf, source: io::Error) -> Self {
        Error::Io {
            path: Some(path),
            source,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Io { .. } => 2,
            Error::InvalidPath { .. } => 1,
            Error::PartialFailure { .. } => 1,
        }
    }
}
