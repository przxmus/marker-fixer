use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarkerFixerError {
    #[error("No input paths provided")]
    NoInputPaths,

    #[error("Path does not exist: {0}")]
    PathMissing(PathBuf),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("External tool error ({tool}): {message}")]
    ExternalTool { tool: &'static str, message: String },

    #[error("Invalid media metadata: {0}")]
    InvalidMetadata(String),

    #[error("Invalid MP4 structure: {0}")]
    InvalidMp4(String),

    #[error("Invalid XMP metadata: {0}")]
    InvalidXmp(String),
}

pub type Result<T> = std::result::Result<T, MarkerFixerError>;

pub trait IoResultExt<T> {
    fn at_path(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoResultExt<T> for std::io::Result<T> {
    fn at_path(self, path: impl Into<PathBuf>) -> Result<T> {
        let path = path.into();
        self.map_err(|source| MarkerFixerError::Io { path, source })
    }
}
