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

        // TODO delete folders up too
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
    if let Some(ext) = p.extension() {
        let ext = ext.to_ascii_lowercase();

        ext == "cook" || ext == "conf" || ext == "yaml" || ext == "yml" || ext == "md" || ext == "cplan"
    } else {
        false
    }
}
