use thiserror::Error;

#[derive(Error,Debug)]
#[derive(uniffi::Error)]
#[uniffi(flat_error)]
pub enum SyncError {
    #[error("IO error {0}")]
    IoError(#[from] std::io::Error),
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
    #[error("Unknown error")]
    Unknown,
}
