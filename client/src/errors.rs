use std::fmt;
use std::error;


#[derive(Debug)]
#[derive(uniffi::Error)]
#[uniffi(flat_error)]
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


// Implement the `Display` trait for `SyncError`
impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::IoError(e) => write!(f, "IO error: {}", e),
            SyncError::NotifyError(e) => write!(f, "Notify error: {}", e),
            SyncError::ConnectionInitError(e) => write!(f, "Connection init error: {}", e),
            SyncError::StripPrefix(e) => write!(f, "Strip prefix error: {}", e),
            SyncError::SystemTime(e) => write!(f, "System time error: {}", e),
            SyncError::Convert(e) => write!(f, "Conversion error: {}", e),
            SyncError::DBQueryError(e) => write!(f, "Database query error: {}", e),
            // Add additional match arms as needed
        }
    }
}

// Implement the `Error` trait for `SyncError`
impl error::Error for SyncError {}

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
