//! Spaces and their shared Files folder. On disk they look exactly as Electron Canopy keeps them
//! (`lib/spaces.cjs`, `lib/files.cjs`), so both versions open the same spaces.

pub mod files;
pub mod spaces;

use std::fmt;

/// An error worded for the person using Canopy.
#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error(error.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Milliseconds since 1970, as JavaScript's Date.now() writes them.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
