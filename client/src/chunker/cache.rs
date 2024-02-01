use std::collections::HashMap;
use std::io::{self};
use bytes::Bytes;

pub trait Cache {
    fn get_chunk(&self, chunk_hash: &str) -> io::Result<Bytes>;
    fn set_chunk(&mut self, chunk_hash: &str, content: Bytes) -> io::Result<()>;
    fn contains_chunk(&self, chunk_hash: &str) -> bool;
}

pub struct InMemoryCache {
    cache: HashMap<String, Bytes>,
}

impl InMemoryCache {
    pub fn new() -> InMemoryCache {
        InMemoryCache {
            cache: HashMap::new(),
        }
    }
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache for InMemoryCache {
    fn get_chunk(&self, chunk_hash: &str) -> io::Result<Bytes> {
        match self.cache.get(chunk_hash) {
            Some(content) => Ok(content.clone()),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Chunk not found in cache",
            )),
        }
    }

    fn set_chunk(&mut self, chunk_hash: &str, content: Bytes) -> io::Result<()> {
        self.cache.insert(chunk_hash.to_string(), content);
        Ok(())
    }

    fn contains_chunk(&self, chunk_hash: &str) -> bool {
        self.cache.contains_key(chunk_hash)
    }
}
