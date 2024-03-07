use std::path::PathBuf;
use tokio::fs::{self, create_dir_all, File};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

use log::trace;
use quick_cache::{sync::Cache, Weighter};
use sha2::{Digest, Sha256};

use crate::errors::SyncError;

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
        let file = File::open(self.full_path(path)).await?;
        let mut reader = BufReader::new(file);
        let mut buffer = Vec::new();
        let mut hashes = Vec::new();

        // TODO should work for binary too
        while reader.read_until(b'\n', &mut buffer).await? > 0 {
            let data: Vec<u8> = buffer.clone();
            let hash = self.hash(&data);
            self.save_chunk(&hash, data)?;
            hashes.push(hash);

            // Clear the buffer for the next line
            buffer.clear();
        }

        Ok(hashes)
    }

    pub fn hash(&self, data: &Vec<u8>) -> String {
        let mut hasher = Sha256::new();

        hasher.update(data);

        let result = hasher.finalize();
        let hex_string = format!("{:x}", result);

        hex_string[0..10].to_string()
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
            create_dir_all(parent).await?;
        }

        let file = File::create(full_path).await?;
        let mut writer = BufWriter::new(file);

        for hash in hashes {
            let chunk = self.cache.get(hash)?;

            writer.write_all(&chunk).await?;
        }

        writer.flush().await?;

        Ok(())
    }

    pub async fn delete(&mut self, path: &str) -> Result<()> {
        trace!("deleting {:?}", path);
        let full_path = self.full_path(path);

        // TODO delete folders up too
        fs::remove_file(full_path).await?;

        Ok(())
    }

    pub fn read_chunk(&self, chunk_hash: &str) -> Result<Vec<u8>> {
        self.cache.get(chunk_hash)
    }

    pub fn save_chunk(&mut self, chunk_hash: &str, content: Vec<u8>) -> Result<()> {
        self.cache.set(chunk_hash, content)
    }

    pub fn check_chunk(&self, chunk_hash: &str) -> Result<bool> {
        if chunk_hash.is_empty() {
            Ok(true)
        } else {
            Ok(self.cache.contains(chunk_hash))
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
