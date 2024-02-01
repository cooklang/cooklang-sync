use std::fs::{File, create_dir_all};
use std::io::{self, prelude::*, BufReader, BufWriter};
use bytes::Bytes;
use sha2::{Sha256, Digest};
use std::path::{PathBuf};

mod cache;
use log::{trace};

pub use cache::{Cache, InMemoryCache};

pub struct Chunker<C: Cache> {
    cache: C,
    base_path: PathBuf,
}

impl<C: Cache> Chunker<C> {
    pub fn new(cache: C, base_path: PathBuf) -> Chunker<C> {
        Chunker { cache, base_path: base_path }
    }

    fn full_path(&self, path: &str) -> PathBuf {
        let mut base = self.base_path.clone();
        base.push(path);
        base
    }

    pub fn hashify(&mut self, path: &str) -> io::Result<Vec<String>> {
        let file = File::open(self.full_path(path))?;
        let mut reader = BufReader::new(file);
        let mut hashes = Vec::new();

        let mut buffer = Vec::new();

        while reader.read_until(b'\n', &mut buffer)? > 0 {
            let data: Bytes = buffer.clone().into();
            let hash = self.hash(&data);
            self.save_chunk(&hash, data);
            hashes.push(hash);

            // Clear the buffer for the next line
            buffer.clear();
        }

        trace!("[chunker] hashes {:?}", hashes);

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
        trace!("[chunker] saving {:?}", path);
        let full_path = self.full_path(path);
        if let Some(parent) = full_path.parent() {
            create_dir_all(parent)?;
        }

        let file = File::create(full_path)?;
        trace!("[chunker] here");
        let mut writer = BufWriter::new(file);


        for hash in hashes {
            let chunk = self.cache.get_chunk(hash)?;
            trace!("[chunker] writing chunk {:?}", chunk);

            writer.write_all(&chunk)?;
        }

        trace!("[chunker] flush {:?}", writer);

        writer.flush()?;

        Ok(())
    }

    pub fn read_chunk(&self, chunk_hash: &str) -> io::Result<Bytes> {
        self.cache.get_chunk(chunk_hash)
    }

    pub fn save_chunk(&mut self, chunk_hash: &str, content: Bytes) -> io::Result<()> {
        self.cache.set_chunk(chunk_hash, content)
    }


    pub fn check_chunk(&self, chunk_hash: &str) -> io::Result<bool> {
        Ok(self.cache.contains_chunk(chunk_hash))
    }
}

