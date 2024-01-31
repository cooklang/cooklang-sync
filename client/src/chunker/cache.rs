use std::collections::HashMap;
use std::io::{self};

pub trait Cache {
    fn get_chunk(&self, chunk_hash: &str) -> io::Result<String>;
    fn set_chunk(&mut self, chunk_hash: String, content: String) -> io::Result<()>;
    fn contains_chunk(&self, chunk_hash: &str) -> bool;
}

pub struct InMemoryCache {
    cache: HashMap<String, String>,
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
    fn get_chunk(&self, chunk_hash: &str) -> io::Result<String> {
        match self.cache.get(chunk_hash) {
            Some(content) => Ok(content.clone()),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Chunk not found in cache",
            )),
        }
    }

    fn set_chunk(&mut self, chunk_hash: String, content: String) -> io::Result<()> {
        self.cache.insert(chunk_hash, content);
        Ok(())
    }

    fn contains_chunk(&self, chunk_hash: &str) -> bool {
        self.cache.contains_key(chunk_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_cache_is_empty() {
        let cache = InMemoryCache::new();
        assert!(cache.cache.is_empty());
    }

    #[test]
    fn test_set_and_get_chunk() {
        let mut cache = InMemoryCache::new();
        let chunk_hash = String::from("hash1");
        let content = String::from("content1");

        assert!(cache.set_chunk(chunk_hash.clone(), content.clone()).is_ok());
        assert_eq!(cache.get_chunk(&chunk_hash).unwrap(), content);
    }

    #[test]
    fn test_get_nonexistent_chunk() {
        let cache = InMemoryCache::new();
        let chunk_hash = String::from("nonexistent");

        assert!(cache.get_chunk(&chunk_hash).is_err());
    }

    #[test]
    fn test_contains_chunk() {
        let mut cache = InMemoryCache::new();
        let chunk_hash = String::from("hash1");
        let content = String::from("content1");

        cache.set_chunk(chunk_hash.clone(), content).unwrap();
        assert!(cache.contains_chunk(&chunk_hash));
    }

    #[test]
    fn test_does_not_contain_chunk() {
        let cache = InMemoryCache::new();
        let chunk_hash = String::from("hash1");

        assert!(!cache.contains_chunk(&chunk_hash));
    }
}
