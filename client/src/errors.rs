#[derive(Debug)]
pub enum SyncError {
    IoError(std::io::Error),
    NotifyError(notify::Error),
    RunMigrationError,
    // StandardError(std::error::Error),
    // Add other error types as needed
}

impl From<std::io::Error> for SyncError {
    fn from(error: std::io::Error) -> Self {
        SyncError::IoError(error)
    }
}

impl From<notify::Error> for SyncError {
    fn from(error: notify::Error) -> Self {
        SyncError::NotifyError(error)
    }
}

// impl From<dyn std::convert::From<Box<dyn std::error::Error + std::marker::Send + Sync>>> for SyncError {
//     fn from(error: dyn std::convert::From<Box<dyn std::error::Error + std::marker::Send + Sync>>) -> Self {
//         SyncError::RunMigrationError
//     }
// }

// impl From<dyn std::error::Error> for SyncError {
//     fn from(error: dyn std::error::Error) -> Self {
//         SyncError::StandardError(error)
//     }
// }

// Implement conversions for other error types as necessary
