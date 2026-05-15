//! Integration tests for `cooklang_sync_client::connection`.

mod common;

use cooklang_sync_client::connection::{get_connection, get_connection_pool};
use diesel::prelude::*;
use tempfile::TempDir;

#[test]
fn get_connection_pool_creates_db_and_runs_migrations() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("fresh.sqlite3");
    assert!(!db_path.exists(), "precondition: DB file does not exist yet");

    let pool = get_connection_pool(db_path.to_str().unwrap())
        .expect("pool creation with migrations should succeed");

    // After pool creation the DB file exists.
    assert!(db_path.exists(), "SQLite file should be created");

    // Migrations should have created the `file_records` table.
    // We probe it with a count query that must succeed against the real schema.
    let conn = &mut get_connection(&pool).expect("checkout connection");
    let count: i64 = diesel::sql_query("SELECT COUNT(*) AS c FROM file_records")
        .load::<RowCount>(conn)
        .expect("migration must have created file_records")
        .first()
        .map(|r| r.c)
        .unwrap_or(-1);
    assert_eq!(count, 0, "fresh DB should have zero rows in file_records");
}

// This test exercises the ConnectionInitError branch by giving get_connection_pool
// an unwritable path. It is marked #[ignore] because r2d2 retries failed connection
// establishment for its default `connection_timeout` (30s) before surfacing the
// error — making the test slow in an ordinary `cargo test` run. Run it explicitly
// with `cargo test -- --ignored` when you want to verify the error path.
#[test]
#[ignore = "slow: r2d2 retries failed establish for ~30s"]
fn get_connection_pool_returns_error_for_unwritable_path() {
    // On macOS, /dev/null is a character device so any sub-path is not a valid
    // directory, causing SQLite to fail to open the file.
    let bogus = "/dev/null/does_not_exist/db.sqlite3";
    let err = get_connection_pool(bogus)
        .err()
        .expect("pool creation should fail on unwritable parent");
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("connection"),
        "expected ConnectionInitError message, got: {msg}"
    );
}

#[test]
fn get_connection_checks_out_multiple_times() {
    let (pool, _dir) = common::fresh_client_pool();
    let a = get_connection(&pool).expect("first checkout");
    drop(a);
    let _b = get_connection(&pool).expect("second checkout after drop");
}

#[derive(QueryableByName)]
struct RowCount {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    c: i64,
}
