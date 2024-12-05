use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum SyncError {
    #[error("IO error in file {path}: {source}")]
    IoError {
        path: String,
        source: std::io::Error,
    },
    #[error("IO error {0}")]
    IoErrorGeneric(#[from] std::io::Error),
    #[error("Notify error {0}")]
    NotifyError(#[from] notify::Error),
    #[error("Strip prefix error {0}")]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error("System time error {0}")]
    SystemTime(#[from] std::time::SystemTimeError),
    #[error("Conversion error {0}")]
    Convert(#[from] std::num::TryFromIntError),
    #[error("Database query error {0}")]
    DBQueryError(#[from] diesel::result::Error),
    #[error("Reqwest error {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Reqwest with middleware error {0}")]
    ReqwestWirhMiddlewareError(#[from] reqwest_middleware::Error),
    #[error("Error sending value to a channel {0}")]
    ChannelSendError(#[from] futures::channel::mpsc::SendError),
    #[error("Connection init error {0}")]
    ConnectionInitError(String),
    #[error("Unauthorized token")]
    Unauthorized,
    #[error("Can't parse the response")]
    BodyExtractError,
    #[error("Can't find in cache")]
    GetFromCacheError,
    #[error("Unlisted file format {0}")]
    UnlistedFileFormat(String),
    #[error("Unknown error")]
    Unknown,
    #[error("Batch download error {0}")]
    BatchDownloadError(String),
}

impl SyncError {
    pub fn from_io_error(path: impl Into<PathBuf>, error: std::io::Error) -> Self {
        SyncError::IoError {
            path: path.into().display().to_string(),
            source: error,
        }
    }
}
