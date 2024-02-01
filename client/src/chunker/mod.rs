use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};
use bytes::Bytes;
use sha2::{Sha256, Digest};

mod cache;

pub use cache::{Cache, InMemoryCache};

pub struct Chunker<C: Cache> {
    cache: C,
}

impl<C: Cache> Chunker<C> {
    pub fn new(cache: C) -> Chunker<C> {
        Chunker { cache }
    }

    pub fn hashify(&mut self, file_path: &str) -> io::Result<Vec<String>> {
        let file = File::open(file_path)?;
        let reader = BufReader::new(file);
        let mut hashes = Vec::new();

        for line in reader.lines() {
            let data: Bytes = line?.into();
            let hash = self.hash(&data);
            self.save_chunk(&hash, data);
            hashes.push(hash);
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

    pub fn save(&mut self, file_path: &str, hashes: Vec<&str>) -> io::Result<()> {
        let file = File::create(file_path)?;
        let mut writer = BufWriter::new(file);

        for hash in hashes {
            let chunk = self.cache.get_chunk(hash)?;

            writer.write_all(&chunk)?;
        }

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

