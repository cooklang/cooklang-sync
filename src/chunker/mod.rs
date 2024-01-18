use std::fs::{File};
use std::io::{self, prelude::*, BufReader};

use blake3::{Hasher};

mod cache;

pub use cache::{Cache, InMemoryCache};

pub struct Chunker<C: Cache> {
    cache: C,
}

impl<C: Cache> Chunker<C> {
    pub fn new(cache: C) -> Chunker<C> {
        Chunker { cache }
    }

    pub fn hashify(&self, file_path: String) -> io::Result<Vec<String>> {
        let file = File::open(file_path)?;
        let reader = BufReader::new(file);
        let mut hashes = Vec::new();

        for line in reader.lines() {
            let data = line?;
            hashes.push(self.hash(data));
        }

        Ok(hashes)
    }

    pub fn hash(&self, data: String) -> String {
        let mut hasher = Hasher::new();

        hasher.update(data.as_bytes());

        format!("{}", hasher.finalize())
    }

    pub fn save(&mut self, file_path: String, hashes: Vec<String>) -> io::Result<()> {
        let mut file = File::create(file_path)?;

        for hash in hashes {
            let content = self.cache.get_chunk(&hash)?;
            writeln!(file, "{}", content)?;
        }

        Ok(())
    }

    pub fn read_chunk(&self, chunk_hash: String) -> io::Result<String> {
        self.cache.get_chunk(&chunk_hash)
    }

    pub fn save_chunk(&mut self, chunk_hash: String, content: String) -> io::Result<()> {
        self.cache.set_chunk(chunk_hash, content)
    }

    pub fn compare_sets(&self, left: Vec<String>, right: Vec<String>) -> bool {
        left == right
    }

    pub fn check_chunk(&self, chunk_hash: String) -> io::Result<bool> {
        Ok(self.cache.contains_chunk(&chunk_hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cache::{InMemoryCache};
    use std::fs::File;
    use std::io::Write;

    // Helper function to create a test file
    fn create_test_file(path: &str, contents: &str) {
        let mut file = File::create(path).unwrap();
        writeln!(file, "{}", contents).unwrap();
    }

    #[test]
    fn test_hashify() {
        let chunker = Chunker::new(InMemoryCache::new());
        create_test_file("tmp/test.txt", "Hello\nWorld");
        let hashes = chunker.hashify("tmp/test.txt".to_string()).unwrap();

        assert_eq!(hashes.len(), 2); // Ensure two lines are hashed
        // Further asserts can be added to check specific hash values
    }

    #[test]
    fn test_save() {
        let mut chunker = Chunker::new(InMemoryCache::new());
        create_test_file("tmp/test_chunk.txt", "Chunk content");

        let hash = chunker.hashify("tmp/test_chunk.txt".to_string()).unwrap()[0].clone();
        chunker.save_chunk(hash.clone(), "Chunk content".to_string()).unwrap();

        let save_result = chunker.save("test_save.txt".to_string(), vec![hash]);
        assert!(save_result.is_ok()); // Check if file is saved without errors
    }

    #[test]
    fn test_read_chunk() {
        let mut chunker = Chunker::new(InMemoryCache::new());
        let chunk_hash = "some_hash_string".to_string();
        let chunk_content = "some content".to_string();

        chunker.save_chunk(chunk_hash.clone(), chunk_content.clone()).unwrap();
        let result = chunker.read_chunk(chunk_hash).unwrap();

        assert_eq!(result, chunk_content); // Check if the content matches
    }

    #[test]
    fn test_save_chunk() {
        let mut chunker = Chunker::new(InMemoryCache::new());
        let chunk_hash = "some_hash_string".to_string();
        let chunk_content = "some content".to_string();

        chunker.save_chunk(chunk_hash.clone(), chunk_content.clone()).unwrap();

        assert!(chunker.cache.contains_chunk(&chunk_hash)); // Check if the chunk is saved
    }

    #[test]
    fn test_compare_sets() {
        let chunker = Chunker::new(InMemoryCache::new());
        let set1 = vec!["hash1".to_string(), "hash2".to_string()];
        let set2 = vec!["hash1".to_string(), "hash2".to_string()];
        let set3 = vec!["hash3".to_string(), "hash4".to_string()];

        assert!(chunker.compare_sets(set1.clone(), set2.clone())); // Check for equality
        assert!(!chunker.compare_sets(set1, set3)); // Check for inequality
    }

    #[test]
    fn test_check_chunk() {
        let mut chunker = Chunker::new(InMemoryCache::new());
        let chunk_hash = "some_hash_string".to_string();
        let chunk_content = "some content".to_string();

        chunker.save_chunk(chunk_hash.clone(), chunk_content).unwrap();
        assert!(chunker.check_chunk(chunk_hash.clone()).unwrap()); // Check if chunk exists
        assert!(!chunker.check_chunk("non_existent_hash".to_string()).unwrap()); // Check for non-existent chunk
    }
}
