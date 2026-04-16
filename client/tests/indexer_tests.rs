//! Integration tests for `cooklang_sync_client::indexer::check_index_once`.
//!
//! Each test creates a fresh `TempDir`, drops files into it, runs
//! `check_index_once`, and asserts the resulting state of `file_records`.

mod common;

use cooklang_sync_client::connection::get_connection;
use cooklang_sync_client::indexer::check_index_once;
use cooklang_sync_client::models::FileRecord;
use cooklang_sync_client::registry;
use cooklang_sync_client::schema::file_records;
use diesel::prelude::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use tempfile::TempDir;

const NS: i32 = 1;

/// Build an empty storage tempdir the indexer can scan.
fn storage_dir() -> TempDir {
    TempDir::new().expect("tempdir")
}

fn write(storage: &TempDir, rel: &str, bytes: &[u8]) -> PathBuf {
    let full = storage.path().join(rel);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full, bytes).unwrap();
    full
}

#[test]
fn check_index_once_records_new_file() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();
    write(&storage, "recipes/soup.cook", b"title: Soup\n");

    let changed = check_index_once(&pool, storage.path(), NS).expect("scan");
    assert!(changed, "new file must cause an update");

    let conn = &mut get_connection(&pool).unwrap();
    let live = registry::non_deleted(conn, NS).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].path, "recipes/soup.cook");
    assert!(!live[0].deleted);
    assert!(live[0].jid.is_none(), "indexer records are always unsynced");
    assert!(live[0].size > 0);
    assert_eq!(live[0].namespace_id, NS, "row must be scoped to the requested namespace");
}

#[test]
fn check_index_once_is_noop_when_nothing_changed() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();
    write(&storage, "a.cook", b"hello");

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());
    assert!(!check_index_once(&pool, storage.path(), NS).unwrap(),
        "second scan with no FS changes must return false");

    let conn = &mut get_connection(&pool).unwrap();
    let rows: i64 = file_records::table
        .count()
        .get_result(conn)
        .unwrap();
    assert_eq!(rows, 1, "no duplicate row appended on no-op scan");
}

#[test]
fn check_index_once_appends_a_new_row_when_file_is_modified() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();
    let path = write(&storage, "a.cook", b"v1");
    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    // Rewrite content with a different size and advance mtime by >=1 second
    // so truncate_to_seconds still produces a distinguishable value.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&path, b"v2-longer").unwrap();

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let rows: Vec<FileRecord> = file_records::table
        .filter(file_records::path.eq("a.cook"))
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load(conn)
        .unwrap();
    assert_eq!(rows.len(), 2, "modified file => new CreateForm appended, not in-place update");
    // The test deliberately changed BOTH the content length and the mtime, so
    // both signals must differ; asserting each catches regressions where only
    // one diff path survives.
    assert_ne!(rows[0].size, rows[1].size);
    assert!(
        rows[1].modified_at > rows[0].modified_at,
        "mtime must strictly advance after the >=1.1s sleep; if equal, either the \
         sleep was too short or truncate_to_seconds collapsed both readings"
    );

    // non_deleted yields the newer row.
    let live = registry::non_deleted(conn, NS).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, rows[1].id);
}

#[test]
fn check_index_once_records_delete_when_file_is_removed() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();
    let path = write(&storage, "gone.cook", b"bye");
    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    fs::remove_file(&path).unwrap();
    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let live = registry::non_deleted(conn, NS).unwrap();
    assert!(live.is_empty(), "removed file must be absent from non_deleted");

    let rows: Vec<FileRecord> = file_records::table
        .filter(file_records::path.eq("gone.cook"))
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load(conn)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(!rows[0].deleted);
    assert!(rows[1].deleted);
}

#[test]
fn check_index_once_skips_ineligible_files_and_recurses_into_subdirs() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();

    // eligible (chunker::is_text / is_binary):
    write(&storage, "top.cook", b"a");
    write(&storage, "nested/dir/inner.md", b"b");
    write(&storage, "photo.jpg", &[0xff, 0xd8, 0xff]);
    // ineligible extensions:
    write(&storage, "notes.txt", b"c");
    write(&storage, "script.rs", b"d");

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let mut paths: Vec<String> = registry::non_deleted(conn, NS)
        .unwrap()
        .into_iter()
        .map(|r| r.path)
        .collect();
    paths.sort();
    assert_eq!(paths, vec![
        "nested/dir/inner.md".to_string(),
        "photo.jpg".to_string(),
        "top.cook".to_string(),
    ]);
}

#[test]
fn check_index_once_indexes_dotfiles_on_the_is_text_allowlist() {
    // Pins current chunker::is_text behavior: `.shopping-list`,
    // `.shopping-checked`, `.bookmarks` are explicitly allowed even though
    // they have no extension and start with a dot.
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();

    write(&storage, ".shopping-list", b"milk");
    write(&storage, ".hidden-random", b"not included");

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let mut paths: Vec<String> = registry::non_deleted(conn, NS)
        .unwrap()
        .into_iter()
        .map(|r| r.path)
        .collect();
    paths.sort();
    assert_eq!(paths, vec![".shopping-list".to_string()]);
}

#[cfg(unix)]
#[test]
fn check_index_once_skips_symlinks() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();

    let target = write(&storage, "real.cook", b"r");
    let link = storage.path().join("link.cook");
    symlink(&target, &link).expect("symlink");

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let mut paths: Vec<String> = registry::non_deleted(conn, NS)
        .unwrap()
        .into_iter()
        .map(|r| r.path)
        .collect();
    paths.sort();
    assert_eq!(paths, vec!["real.cook".to_string()],
        "symlink entry must be skipped by filter_eligible");
}
