use std::collections::HashMap;
use std::fs::{File, create_dir_all};
use std::io::{self, prelude::*, BufReader, BufWriter};
use std::path::{PathBuf};
use std::fs;

use bytes::Bytes;
use sha2::{Sha256, Digest};

use log::{trace};

use quick_cache::{Weighter, sync::Cache};

pub struct Chunker {
    cache: InMemoryCache,
    base_path: PathBuf,
}


impl Chunker {
    pub fn new(cache: InMemoryCache, base_path: PathBuf) -> Chunker {
        Chunker { cache, base_path }
    }

    fn full_path(&self, path: &str) -> PathBuf {
        let mut base = self.base_path.clone();
        base.push(path);
        base
    }

    pub fn hashify(&mut self, path: &str) -> io::Result<Vec<String>> {
        let file = File::open(self.full_path(path))?;
        let mut reader = BufReader::new(file);
        let mut buffer = Vec::new();

        let mut hashes = Vec::new();

        // TODO should work for
        while reader.read_until(b'\n', &mut buffer)? > 0 {
            let data: Bytes = buffer.clone().into();
            let hash = self.hash(&data);
            self.save_chunk(&hash, data);
            hashes.push(hash);

            // Clear the buffer for the next line
            buffer.clear();
        }

        Ok(hashes)
    }

    pub fn hash(&self, data: &Bytes) -> String {
        let mut hasher = Sha256::new();

        hasher.update(data);

        let result = hasher.finalize();
        let hex_string = format!("{:x}", result);

        hex_string[0..10].to_string()
    }

    pub fn save(&mut self, path: &str, hashes: Vec<&str>) -> io::Result<()> {
        trace!("saving {:?}", path);
        let full_path = self.full_path(path);
        if let Some(parent) = full_path.parent() {
            create_dir_all(parent)?;
        }

        let file = File::create(full_path)?;
        let mut writer = BufWriter::new(file);


        for hash in hashes {
            let chunk = self.cache.get(hash)?;

            writer.write_all(&chunk)?;
        }

        writer.flush()?;

        Ok(())
    }

    pub fn delete(&mut self, path: &str) -> io::Result<()> {
        trace!("deleting {:?}", path);
        let full_path = self.full_path(path);

        // TODO delete folders up too
        fs::remove_file(full_path)?;

        Ok(())
    }

    pub fn read_chunk(&self, chunk_hash: &str) -> io::Result<Bytes> {
        self.cache.get(chunk_hash)
    }

    pub fn save_chunk(&mut self, chunk_hash: &str, content: Bytes) -> io::Result<()> {
        self.cache.set(chunk_hash, content)
    }


    pub fn check_chunk(&self, chunk_hash: &str) -> io::Result<bool> {
        Ok(self.cache.contains(chunk_hash))
    }
}


#[derive(Clone)]
pub struct BytesWeighter;

impl Weighter<String, Bytes> for BytesWeighter {
    fn weight(&self, _key: &String, val: &Bytes) -> u32 {
        // Be cautions out about zero weights!
        val.len().clamp(1, u32::MAX as usize) as u32
    }
}


pub struct InMemoryCache {
    cache: Cache<String, Bytes, BytesWeighter>
}

impl InMemoryCache {
    pub fn new(total_keys: usize, total_weight: u64) -> InMemoryCache {
        InMemoryCache {
            cache: Cache::with_weighter(total_keys, total_weight, BytesWeighter)
        }
    }

    fn get(&self, chunk_hash: &str) -> io::Result<Bytes> {
        match self.cache.get(chunk_hash) {
            Some(content) => Ok(content.clone()),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Chunk not found in cache",
            )),
        }
    }

    fn set(&mut self, chunk_hash: &str, content: Bytes) -> io::Result<()> {
        self.cache.insert(chunk_hash.to_string(), content);
        Ok(())
    }

    fn contains(&self, chunk_hash: &str) -> bool {
        match self.cache.get(chunk_hash) {
            Some(content) => true,
            None => false,
        }
    }
}
