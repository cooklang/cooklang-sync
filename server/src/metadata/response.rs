use rocket::serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub(crate) enum CommitResultStatus {
    Success(i32),
    NeedChunks(String),
}
