use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "ffi", derive(uniffi::Error))]
#[cfg_attr(feature = "ffi", uniffi(flat_error))]
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
    #[error("Unknown error: {0}")]
    Unknown(String),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_from_conversion_preserves_message() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "boom");
        let err: SyncError = io.into();
        let msg = format!("{err}");
        assert!(msg.contains("boom"), "wrapped IO error message preserved: {msg}");
        assert!(matches!(err, SyncError::IoErrorGeneric(_)));
    }

    #[test]
    fn from_io_error_helper_attaches_path() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let err = SyncError::from_io_error("/some/path", io);
        let msg = format!("{err}");
        assert!(msg.contains("/some/path"), "path should be in message: {msg}");
        assert!(msg.contains("nope"), "source cause should be in message: {msg}");
        assert!(matches!(err, SyncError::IoError { .. }));
    }

    #[test]
    fn unauthorized_has_stable_display() {
        let err = SyncError::Unauthorized;
        assert_eq!(format!("{err}"), "Unauthorized token");
    }

    #[test]
    fn unknown_variant_includes_context() {
        let err = SyncError::Unknown("xyz".into());
        assert!(format!("{err}").contains("xyz"));
    }
}
