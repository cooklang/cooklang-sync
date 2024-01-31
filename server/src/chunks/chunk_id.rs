use std::borrow::Cow;
use std::path::{Path, PathBuf};

use rocket::request::FromParam;

/// A _probably_ unique paste ID.
#[derive(UriDisplayPath)]
pub struct ChunkId<'a>(Cow<'a, str>);

impl ChunkId<'_> {

    /// Returns the path to the paste in `upload/` corresponding to this ID.
    pub fn file_path(&self) -> PathBuf {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/", "upload");
        Path::new(root).join(self.0.as_ref())
    }
}

/// Returns an instance of `ChunkId` if the path segment is a valid ID.
/// Otherwise returns the invalid ID as the `Err` value.
impl<'a> FromParam<'a> for ChunkId<'a> {
    type Error = &'a str;

    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        param.chars().all(|c| c.is_ascii_alphanumeric())
            .then(|| ChunkId(param.into()))
            .ok_or(param)
    }
}
