use thiserror::Error;

#[derive(Error,Debug)]
#[derive(uniffi::Error)]
#[uniffi(flat_error)]
pub enum SyncError {
    #[error("IO error")]
    IoError(#[from] std::io::Error),
    #[error("Notify error")]
    NotifyError(#[from] notify::Error),
    #[error("Strip prefix error")]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error("System time error")]
    SystemTime(#[from] std::time::SystemTimeError),
    #[error("Conversion error")]
    Convert(#[from] std::num::TryFromIntError),
    #[error("Database query error")]
    DBQueryError(#[from] diesel::result::Error),
    #[error("Reqwest error")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Error sending value to a channel")]
    ChannelSendError(#[from] futures::channel::mpsc::SendError),
    #[error("Connection init error")]
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
