use std::borrow::Cow;
use std::path::{Path, PathBuf};

use rocket::request::FromParam;


/// A _probably_ unique chunk ID.
#[derive(UriDisplayPath)]
pub struct ChunkId<'a>(pub(crate) Cow<'a, str>);

impl ChunkId<'_> {
    /// Returns the path to the chunk in `upload/` corresponding to this ID.
    pub fn file_path(&self) -> PathBuf {
        // TODO make parameter
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/", "upload");
        let id_str = self.0.as_ref();

        // Ensure the ID is long enough
        if id_str.len() < 2 {
            panic!("Chunk ID is too short, must be at least 2 characters long");
        }

        let first_char = &id_str[0..1];
        let second_char = &id_str[1..2];

        Path::new(root).join(first_char).join(second_char).join(id_str)
    }

    /// Returns the path to the chunk in `upload/` corresponding to this ID.
    pub fn is_present(&self) -> bool {
        self.file_path().exists()
    }
}

/// Returns an instance of `ChunkId` if the path segment is a valid ID.
/// Otherwise returns the invalid ID as the `Err` value.
impl<'a> FromParam<'a> for ChunkId<'a> {
    type Error = &'a str;

    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        param
            .chars()
            .all(|c| c.is_ascii_alphanumeric())
            .then(|| ChunkId(param.into()))
            .ok_or(param)
    }
}

impl<'a> From<&'a str> for ChunkId<'a> {
    fn from(file_name: &'a str) -> Self {
        // Convert FileName to a string slice (&str)

        // Convert &str to Cow<str>
        ChunkId(Cow::from(file_name))
    }
}

