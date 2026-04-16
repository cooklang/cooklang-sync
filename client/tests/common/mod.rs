//! Shared helpers for integration tests.
//!
//! This module is included by each integration test via `mod common;`.
//! Every helper returns values paired with the `TempDir` that backs them
//! so callers can keep the directory alive for the test's lifetime.

#![allow(dead_code)] // Not every test file uses every helper.

use cooklang_sync_client::connection::{get_connection_pool, ConnectionPool};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

/// Build a fresh on-disk SQLite pool with all migrations applied.
/// Returns the pool and the backing `TempDir` (keep it alive).
pub fn fresh_client_pool() -> (ConnectionPool, TempDir) {
    let dir = TempDir::new().expect("create tempdir");
    let db_path = dir.path().join("client.sqlite3");
    let db_path_str = db_path.to_str().expect("tempdir path is utf-8").to_string();
    let pool = get_connection_pool(&db_path_str).expect("build connection pool");
    (pool, dir)
}

/// Build a tempdir pre-populated with files at relative paths.
/// Example: `tempdir_with_files(&[("recipes/a.cook", b"Eggs\n")]).await`
pub async fn tempdir_with_files(files: &[(&str, &[u8])]) -> TempDir {
    let dir = TempDir::new().expect("create tempdir");
    for (rel, bytes) in files {
        let path: PathBuf = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.expect("mkdir -p");
        }
        fs::write(&path, bytes).await.expect("write file");
    }
    dir
}

/// Mint a JWT signed with the test secret `"secret"` containing the given uid.
/// Expiration is set far in the future (year 2099).
/// Matches the client's `extract_uid_from_jwt` expectations (HS256, `uid` claim).
pub fn sample_jwt(uid: i32) -> String {
    // Header: {"alg":"HS256","typ":"JWT"} -> URL-safe no-pad base64
    let header = base64_url_nopad(br#"{"alg":"HS256","typ":"JWT"}"#);
    // Payload: {"uid":<uid>,"exp":4102444800}  (2100-01-01)
    let payload_json = format!(r#"{{"uid":{},"exp":4102444800}}"#, uid);
    let payload = base64_url_nopad(payload_json.as_bytes());
    // `extract_uid_from_jwt` does NOT verify the signature, so any placeholder works.
    let signature = base64_url_nopad(b"test-signature-unverified");
    format!("{}.{}.{}", header, payload, signature)
}

fn base64_url_nopad(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(bytes)
}

use cooklang_sync_client::chunker::{Chunker, InMemoryCache};

/// Bundle of the three things every syncer integration test needs:
/// a fresh SQLite pool, a tempdir root, and a `Chunker` rooted at that dir.
///
/// The tempdir is returned separately (not moved into the `Chunker`) so the
/// test can write fixture files into it before invoking the syncer.
pub struct ClientBase {
    pub pool: cooklang_sync_client::connection::ConnectionPool,
    pub dir: TempDir,
    pub chunker: Chunker,
}

pub fn client_base() -> ClientBase {
    let (pool, dir) = fresh_client_pool();
    let cache = InMemoryCache::new(100, 10_000_000);
    let chunker = Chunker::new(cache, dir.path().to_path_buf());
    ClientBase { pool, dir, chunker }
}
