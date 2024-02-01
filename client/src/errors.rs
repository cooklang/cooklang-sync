#[derive(Debug)]
pub enum SyncError {
    IoError(std::io::Error),
    NotifyError(notify::Error),
    ConnectionInitError(String),
    StripPrefix(std::path::StripPrefixError),
    SystemTime(std::time::SystemTimeError),
    Convert(std::num::TryFromIntError),
    DBQueryError(diesel::result::Error),

    // StandardError(std::error::Error),
    // Add other error types as needed
}

impl From<std::io::Error> for SyncError {
    fn from(err: std::io::Error) -> Self {
        SyncError::IoError(err)
    }
}

impl From<notify::Error> for SyncError {
    fn from(err: notify::Error) -> Self {
        SyncError::NotifyError(err)
    }
}

impl From<std::path::StripPrefixError> for SyncError {
    fn from(err: std::path::StripPrefixError) -> SyncError {
        SyncError::StripPrefix(err)
    }
}

impl From<std::time::SystemTimeError> for SyncError {
    fn from(err: std::time::SystemTimeError) -> SyncError {
        SyncError::SystemTime(err)
    }
}

impl From<std::num::TryFromIntError> for SyncError {
    fn from(err: std::num::TryFromIntError) -> SyncError {
        SyncError::Convert(err)
    }
}

impl From<diesel::result::Error> for SyncError {
    fn from(err: diesel::result::Error) -> SyncError {
        SyncError::DBQueryError(err)
    }
}
