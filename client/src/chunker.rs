use log::trace;
use quick_cache::{sync::Cache, Weighter};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs::{self, create_dir_all, File};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};

use crate::errors::SyncError;

const BINARY_CHUNK_SIZE: usize = 1_024 * 1_024; // 1 MB
const BINARY_HASH_SIZE: usize = 32;
const TEXT_HASH_SIZE: usize = 10;

pub struct Chunker {
    cache: InMemoryCache,
    base_path: PathBuf,
}

type Result<T, E = SyncError> = std::result::Result<T, E>;

impl Chunker {
    pub fn new(cache: InMemoryCache, base_path: PathBuf) -> Chunker {
        Chunker { cache, base_path }
    }

    fn full_path(&self, path: &str) -> PathBuf {
        let mut base = self.base_path.clone();
        base.push(path);
        base
    }

    pub async fn hashify(&mut self, path: &str) -> Result<Vec<String>> {
        let p = Path::new(path);

        // TODO probably there's a better way to check if file is binary
        if is_text(p) {
            self.hashify_text(path).await
        } else if is_binary(p) {
            self.hashify_binary(path).await
        } else {
            Err(SyncError::UnlistedFileFormat(path.to_string()))
        }
    }

    async fn hashify_binary(&mut self, path: &str) -> Result<Vec<String>> {
        let file = File::open(self.full_path(path))
            .await
            .map_err(|e| SyncError::from_io_error(path, e))?;
        let mut reader = BufReader::new(file);
        let mut hashes = Vec::new();
        let mut buffer = vec![0u8; BINARY_CHUNK_SIZE];

        loop {
            let bytes_read = reader
                .read(&mut buffer)
                .await
                .map_err(|e| SyncError::from_io_error(path, e))?;
            if bytes_read == 0 {
                break;
            }

            let data = &buffer[..bytes_read].to_vec();
            let hash = self.hash(data, BINARY_HASH_SIZE);
            self.save_chunk(&hash, data.to_vec())?;
            hashes.push(hash);
        }

        Ok(hashes)
    }

    async fn hashify_text(&mut self, path: &str) -> Result<Vec<String>> {
        let file = File::open(self.full_path(path))
            .await
            .map_err(|e| SyncError::from_io_error(path, e))?;
        let mut reader = BufReader::new(file);
        let mut buffer = Vec::new();
        let mut hashes = Vec::new();

        while reader
            .read_until(b'\n', &mut buffer)
            .await
            .map_err(|e| SyncError::from_io_error(path, e))?
            > 0
        {
            let data: Vec<u8> = buffer.clone();
            let hash = self.hash(&data, TEXT_HASH_SIZE);
            self.save_chunk(&hash, data)?;
            hashes.push(hash);

            // Clear the buffer for the next line
            buffer.clear();
        }

        Ok(hashes)
    }

    pub fn hash(&self, data: &Vec<u8>, size: usize) -> String {
        let mut hasher = Sha256::new();

        hasher.update(data);

        let result = hasher.finalize();
        let hex_string = format!("{:x}", result);

        hex_string[0..size].to_string()
    }

    pub fn exists(&mut self, path: &str) -> bool {
        let full_path = self.full_path(path);

        full_path.exists()
    }

    // TODO can be a problem as it expects cache to contain all chunks
    pub async fn save(&mut self, path: &str, hashes: Vec<&str>) -> Result<()> {
        trace!("saving {:?}", path);
        let full_path = self.full_path(path);

        if let Some(parent) = full_path.parent() {
            create_dir_all(parent)
                .await
                .map_err(|e| SyncError::from_io_error(path, e))?;
        }

        let file = File::create(full_path)
            .await
            .map_err(|e| SyncError::from_io_error(path, e))?;
        let mut writer = BufWriter::new(file);

        for hash in hashes {
            let chunk = self.cache.get(hash)?;

            writer
                .write_all(&chunk)
                .await
                .map_err(|e| SyncError::from_io_error(path, e))?;
        }

        writer
            .flush()
            .await
            .map_err(|e| SyncError::from_io_error(path, e))?;

        Ok(())
    }

    pub async fn delete(&mut self, path: &str) -> Result<()> {
        trace!("deleting {:?}", path);
        let full_path = self.full_path(path);

        // TODO delete folders up too if empty
        fs::remove_file(full_path)
            .await
            .map_err(|e| SyncError::from_io_error(path, e))?;

        Ok(())
    }

    pub fn read_chunk(&self, chunk_hash: &str) -> Result<Vec<u8>> {
        self.cache.get(chunk_hash)
    }

    pub fn save_chunk(&mut self, chunk_hash: &str, content: Vec<u8>) -> Result<()> {
        self.cache.set(chunk_hash, content)
    }

    pub fn check_chunk(&self, chunk_hash: &str) -> bool {
        if chunk_hash.is_empty() {
            true
        } else {
            self.cache.contains(chunk_hash)
        }
    }
}

#[derive(Clone)]
pub struct BytesWeighter;

impl Weighter<String, Vec<u8>> for BytesWeighter {
    fn weight(&self, _key: &String, val: &Vec<u8>) -> u32 {
        // Be cautions out about zero weights!
        val.len().clamp(1, u32::MAX as usize) as u32
    }
}

pub struct InMemoryCache {
    cache: Cache<String, Vec<u8>, BytesWeighter>,
}

impl InMemoryCache {
    pub fn new(total_keys: usize, total_weight: u64) -> InMemoryCache {
        InMemoryCache {
            cache: Cache::with_weighter(total_keys, total_weight, BytesWeighter),
        }
    }

    fn get(&self, chunk_hash: &str) -> Result<Vec<u8>> {
        if chunk_hash.is_empty() {
            return Ok(vec![]);
        }

        match self.cache.get(chunk_hash) {
            Some(content) => Ok(content.clone()),
            None => Err(SyncError::GetFromCacheError),
        }
    }

    fn set(&mut self, chunk_hash: &str, content: Vec<u8>) -> Result<()> {
        // trace!("setting hash {:?} data  {:?}", chunk_hash, content.len());
        self.cache.insert(chunk_hash.to_string(), content);
        Ok(())
    }

    fn contains(&self, chunk_hash: &str) -> bool {
        match self.cache.get(chunk_hash) {
            Some(_content) => true,
            None => false,
        }
    }
}

pub fn is_binary(p: &Path) -> bool {
    if let Some(ext) = p.extension() {
        let ext = ext.to_ascii_lowercase();

        ext == "jpg" || ext == "jpeg" || ext == "png"
    } else {
        false
    }
}

pub fn is_text(p: &Path) -> bool {
    // Check for specific filenames without extensions
    if let Some(file_name) = p.file_name() {
        let file_name_str = file_name.to_string_lossy();
        if file_name_str == ".shopping-list"
            || file_name_str == ".shopping-checked"
            || file_name_str == ".bookmarks"
        {
            return true;
        }
    }

    // Check for file extensions
    if let Some(ext) = p.extension() {
        let ext = ext.to_ascii_lowercase();

        ext == "cook"
            || ext == "conf"
            || ext == "yaml"
            || ext == "yml"
            || ext == "md"
            || ext == "menu"
            || ext == "jinja"
            || ext == "j2"
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn test_is_binary_with_jpg() {
        let path = Path::new("image.jpg");
        assert!(is_binary(path));
    }

    #[test]
    fn test_is_binary_with_jpeg() {
        let path = Path::new("image.JPEG");
        assert!(is_binary(path));
    }

    #[test]
    fn test_is_binary_with_png() {
        let path = Path::new("image.png");
        assert!(is_binary(path));
    }

    #[test]
    fn test_is_binary_returns_false_for_text() {
        let path = Path::new("recipe.cook");
        assert!(!is_binary(path));
    }

    #[test]
    fn test_is_text_with_cook_extension() {
        let path = Path::new("recipe.cook");
        assert!(is_text(path));
    }

    #[test]
    fn test_is_text_with_md_extension() {
        let path = Path::new("README.md");
        assert!(is_text(path));
    }

    #[test]
    fn test_is_text_with_yaml_extension() {
        let path = Path::new("config.yaml");
        assert!(is_text(path));
    }

    #[test]
    fn test_is_text_with_yml_extension() {
        let path = Path::new("config.yml");
        assert!(is_text(path));
    }

    #[test]
    fn test_is_text_with_special_filenames() {
        assert!(is_text(Path::new(".shopping-list")));
        assert!(is_text(Path::new(".shopping-checked")));
        assert!(is_text(Path::new(".bookmarks")));
    }

    #[test]
    fn test_is_text_returns_false_for_unknown() {
        let path = Path::new("file.unknown");
        assert!(!is_text(path));
    }

    #[test]
    fn test_hash_consistency() {
        let cache = InMemoryCache::new(100, 1000);
        let chunker = Chunker::new(cache, PathBuf::from("/tmp"));

        let data = b"Hello, World!".to_vec();
        let hash1 = chunker.hash(&data, 10);
        let hash2 = chunker.hash(&data, 10);

        // Same input should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_different_data_produces_different_hash() {
        let cache = InMemoryCache::new(100, 1000);
        let chunker = Chunker::new(cache, PathBuf::from("/tmp"));

        let data1 = b"Hello, World!".to_vec();
        let data2 = b"Goodbye, World!".to_vec();

        let hash1 = chunker.hash(&data1, 10);
        let hash2 = chunker.hash(&data2, 10);

        // Different input should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_respects_size_parameter() {
        let cache = InMemoryCache::new(100, 1000);
        let chunker = Chunker::new(cache, PathBuf::from("/tmp"));

        let data = b"Hello, World!".to_vec();
        let hash_short = chunker.hash(&data, 5);
        let hash_long = chunker.hash(&data, 10);

        assert_eq!(hash_short.len(), 5);
        assert_eq!(hash_long.len(), 10);
        // Shorter hash should be prefix of longer hash
        assert!(hash_long.starts_with(&hash_short));
    }

    #[test]
    fn test_inmemory_cache_set_and_get() {
        let mut cache = InMemoryCache::new(100, 1000);

        let hash = "testhash123";
        let data = vec![1, 2, 3, 4, 5];

        cache.set(hash, data.clone()).unwrap();
        let retrieved = cache.get(hash).unwrap();

        assert_eq!(data, retrieved);
    }

    #[test]
    fn test_inmemory_cache_get_nonexistent() {
        let cache = InMemoryCache::new(100, 1000);

        let result = cache.get("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_inmemory_cache_contains() {
        let mut cache = InMemoryCache::new(100, 1000);

        let hash = "testhash456";
        let data = vec![1, 2, 3];

        assert!(!cache.contains(hash));
        cache.set(hash, data).unwrap();
        assert!(cache.contains(hash));
    }

    #[test]
    fn test_inmemory_cache_empty_hash() {
        let cache = InMemoryCache::new(100, 1000);

        // Empty hash should return empty vector
        let result = cache.get("").unwrap();
        assert_eq!(result, Vec::<u8>::new());
    }

    #[test]
    fn test_chunker_check_chunk_empty_hash() {
        let cache = InMemoryCache::new(100, 1000);
        let chunker = Chunker::new(cache, PathBuf::from("/tmp"));

        // Empty hash should return true
        assert!(chunker.check_chunk(""));
    }

    #[test]
    fn test_chunker_check_chunk_existing() {
        let mut cache = InMemoryCache::new(100, 1000);
        cache.set("existinghash", vec![1, 2, 3]).unwrap();
        let chunker = Chunker::new(cache, PathBuf::from("/tmp"));

        assert!(chunker.check_chunk("existinghash"));
    }

    #[test]
    fn test_chunker_check_chunk_nonexistent() {
        let cache = InMemoryCache::new(100, 1000);
        let chunker = Chunker::new(cache, PathBuf::from("/tmp"));

        assert!(!chunker.check_chunk("nonexistent"));
    }

    #[tokio::test]
    async fn test_chunker_hashify_text_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let cache = InMemoryCache::new(1000, 100000);
        let mut chunker = Chunker::new(cache, temp_dir.path().to_path_buf());

        // Create a test file
        let test_file = "test.cook";
        let content = "Line 1\nLine 2\nLine 3\n";
        let mut file = File::create(temp_dir.path().join(test_file))
            .await
            .unwrap();
        file.write_all(content.as_bytes()).await.unwrap();
        file.flush().await.unwrap();

        // Hashify the file
        let hashes = chunker.hashify(test_file).await.unwrap();

        // Should have 3 hashes (one per line)
        assert_eq!(hashes.len(), 3);

        // Verify all chunks are in cache
        for hash in &hashes {
            assert!(chunker.check_chunk(hash));
        }
    }

    #[tokio::test]
    async fn test_chunker_save_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let cache = InMemoryCache::new(1000, 100000);
        let mut chunker = Chunker::new(cache, temp_dir.path().to_path_buf());

        // Save some chunks to cache
        let chunk1 = b"Hello ".to_vec();
        let chunk2 = b"World!".to_vec();
        let hash1 = chunker.hash(&chunk1, 10);
        let hash2 = chunker.hash(&chunk2, 10);

        chunker.save_chunk(&hash1, chunk1).unwrap();
        chunker.save_chunk(&hash2, chunk2).unwrap();

        // Save to file
        let test_file = "output.txt";
        chunker
            .save(test_file, vec![&hash1, &hash2])
            .await
            .unwrap();

        // Verify file exists
        assert!(chunker.exists(test_file));

        // Read file content
        let content = tokio::fs::read(temp_dir.path().join(test_file))
            .await
            .unwrap();
        assert_eq!(content, b"Hello World!");
    }

    #[tokio::test]
    async fn test_chunker_delete() {
        let temp_dir = TempDir::new().unwrap();
        let cache = InMemoryCache::new(1000, 100000);
        let mut chunker = Chunker::new(cache, temp_dir.path().to_path_buf());

        // Create a test file
        let test_file = "to_delete.txt";
        let mut file = File::create(temp_dir.path().join(test_file))
            .await
            .unwrap();
        file.write_all(b"test content").await.unwrap();
        file.flush().await.unwrap();

        assert!(chunker.exists(test_file));

        // Delete the file
        chunker.delete(test_file).await.unwrap();

        // Verify file doesn't exist
        assert!(!chunker.exists(test_file));
    }

    #[test]
    fn test_bytes_weighter() {
        let weighter = BytesWeighter;

        let key = "test".to_string();
        let small_val = vec![1, 2, 3];
        let large_val = vec![0u8; 1000];

        assert_eq!(weighter.weight(&key, &small_val), 3);
        assert_eq!(weighter.weight(&key, &large_val), 1000);
    }

    #[test]
    fn test_bytes_weighter_empty_vec() {
        let weighter = BytesWeighter;

        let key = "test".to_string();
        let empty_val = vec![];

        // Should clamp to minimum of 1
        assert_eq!(weighter.weight(&key, &empty_val), 1);
    }
}
