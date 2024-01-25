
#[derive(Debug)]
pub enum MyError {
    IoError(std::io::Error),
    NotifyError(notify::Error),
    // StandardError(std::error::Error),
    // Add other error types as needed
}

impl From<std::io::Error> for MyError {
    fn from(error: std::io::Error) -> Self {
        MyError::IoError(error)
    }
}


impl From<notify::Error> for MyError {
    fn from(error: notify::Error) -> Self {
        MyError::NotifyError(error)
    }
}

// impl From<dyn std::error::Error> for MyError {
//     fn from(error: dyn std::error::Error) -> Self {
//         MyError::StandardError(error)
//     }
// }

// Implement conversions for other error types as necessary
