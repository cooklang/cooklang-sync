use std::borrow::Cow;
use std::env;
use std::path::{Component, Path, PathBuf};

use rocket::form::{self, FromFormField, ValueField};
use rocket::request::FromParam;

/// Maximum accepted length of a chunk id. Clients send truncated hex SHA-256
/// digests, so anything longer than a full digest is never legitimate.
pub const MAX_CHUNK_ID_LEN: usize = 64;

/// A _probably_ unique chunk ID.
///
/// A `ChunkId` can only be constructed from a validated string (see
/// [`ChunkId::parse`]): either empty (the "empty chunk" marker) or 1..=64
/// ASCII alphanumeric characters. This guarantees that [`ChunkId::file_path`]
/// can never escape the upload directory.
#[derive(UriDisplayPath, PartialEq, Debug, Clone)]
pub struct ChunkId<'a>(Cow<'a, str>);

/// Returns true if `id` is an acceptable chunk id.
pub fn is_valid_chunk_id(id: &str) -> bool {
    id.len() <= MAX_CHUNK_ID_LEN && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

impl<'a> ChunkId<'a> {
    /// Validates `id` and wraps it into a `ChunkId`.
    pub fn parse(id: &'a str) -> Option<Self> {
        is_valid_chunk_id(id).then_some(ChunkId(Cow::Borrowed(id)))
    }

    /// Returns the path to the chunk in `upload/` corresponding to this ID.
    pub fn file_path(&self) -> PathBuf {
        let root = env::var("UPLOAD_DIR").unwrap_or(String::from("./upload"));
        let id_str = self.id();

        // Defense in depth: construction already guarantees this.
        assert!(is_valid_chunk_id(id_str), "invalid chunk id");

        let relative = if id_str.len() < 2 {
            Path::new("null").join(id_str)
        } else {
            Path::new(&id_str[0..1]).join(&id_str[1..2]).join(id_str)
        };

        // Defense in depth: the relative part must only consist of plain
        // components so that joining it can never leave `root`.
        assert!(
            relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
            "chunk path escapes upload dir"
        );

        Path::new(&root).join(relative)
    }

    pub fn id(&self) -> &str {
        self.0.as_ref()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns whether the chunk is already stored on the server.
    pub fn is_present(&self) -> bool {
        if self.is_empty() {
            return true;
        }

        self.file_path().exists()
    }
}

/// Returns an instance of `ChunkId` if the path segment is a valid ID.
/// Otherwise returns the invalid ID as the `Err` value.
impl<'a> FromParam<'a> for ChunkId<'a> {
    type Error = &'a str;

    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        ChunkId::parse(param).ok_or(param)
    }
}

#[rocket::async_trait]
impl<'v> FromFormField<'v> for ChunkId<'v> {
    fn from_value(field: ValueField<'v>) -> form::Result<'v, Self> {
        ChunkId::parse(field.value)
            .ok_or_else(|| form::Error::validation("invalid chunk id").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_hex_ids() {
        assert!(ChunkId::parse("").is_some());
        assert!(ChunkId::parse("a").is_some());
        assert!(ChunkId::parse("0123abcdef").is_some());
        assert!(ChunkId::parse(&"f".repeat(64)).is_some());
    }

    #[test]
    fn rejects_traversal_and_junk() {
        for bad in [
            "..",
            "../x",
            "../../etc/passwd",
            "/etc/passwd",
            "/abs/path/OUTSIDE.txt",
            "a/b",
            "a\\b",
            ".",
            "ab.c",
            "ab c",
            "ab\0c",
            "ä",
        ] {
            assert!(ChunkId::parse(bad).is_none(), "accepted {bad:?}");
        }
        assert!(ChunkId::parse(&"a".repeat(65)).is_none());
    }

    #[test]
    fn file_path_stays_under_root() {
        let id = ChunkId::parse("abcdef").unwrap();
        let p = id.file_path();
        assert!(p.ends_with("a/b/abcdef"));
        let short = ChunkId::parse("z").unwrap();
        assert!(short.file_path().ends_with("null/z"));
    }

    #[test]
    fn from_param_validates() {
        assert!(ChunkId::from_param("abc123").is_ok());
        assert!(ChunkId::from_param("..").is_err());
    }
}
