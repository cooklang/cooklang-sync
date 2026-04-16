//! Integration tests for `cooklang_sync_client::registry`.

mod common;

use cooklang_sync_client::connection::get_connection;
use cooklang_sync_client::models::{CreateForm, DeleteForm, FileRecord};
use cooklang_sync_client::registry;
use cooklang_sync_client::schema::file_records;
use diesel::prelude::*;
use time::OffsetDateTime;

/// Build a `CreateForm` with a deterministic `modified_at` (whole seconds) so
/// equality comparisons via `PartialEq<CreateForm> for FileRecord` are stable.
fn sample_create(path: &str, size: i64, ns: i32) -> CreateForm {
    CreateForm {
        jid: None,
        path: path.to_string(),
        deleted: false,
        size,
        modified_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        namespace_id: ns,
    }
}

fn sample_delete(from: &FileRecord) -> DeleteForm {
    DeleteForm {
        path: from.path.clone(),
        jid: None,
        size: from.size,
        modified_at: from.modified_at,
        deleted: true,
        namespace_id: from.namespace_id,
    }
}

#[test]
fn create_inserts_rows_and_returns_count() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    let forms = vec![
        sample_create("a.cook", 10, 1),
        sample_create("b.cook", 20, 1),
    ];
    let n = registry::create(conn, &forms).expect("create should insert");
    assert_eq!(n, 2);

    let all: Vec<FileRecord> = file_records::table
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load(conn)
        .expect("load all");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].path, "a.cook");
    assert_eq!(all[0].size, 10);
    assert!(all[0].jid.is_none(), "new rows must have no jid yet");
    assert!(all[0].id < all[1].id, "id must be monotonic");
}

#[test]
fn update_jid_sets_jid_and_preserves_other_columns() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    registry::create(conn, &vec![sample_create("a.cook", 42, 1)]).unwrap();

    let row: FileRecord = file_records::table
        .select(FileRecord::as_select())
        .first(conn)
        .expect("row");
    assert!(row.jid.is_none());
    let original_path = row.path.clone();
    let original_size = row.size;
    let original_mtime = row.modified_at;

    let n = registry::update_jid(conn, &row, 7).expect("update_jid");
    assert_eq!(n, 1);

    let after: FileRecord = file_records::table
        .select(FileRecord::as_select())
        .first(conn)
        .expect("reload");
    assert_eq!(after.jid, Some(7));
    assert_eq!(after.path, original_path);
    assert_eq!(after.size, original_size);
    assert_eq!(after.modified_at, original_mtime);
    assert_eq!(after.deleted, false);
}

#[test]
fn delete_appends_tombstone_row_rather_than_updating() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    registry::create(conn, &vec![sample_create("a.cook", 10, 1)]).unwrap();
    let live: FileRecord = file_records::table
        .select(FileRecord::as_select())
        .first(conn)
        .unwrap();
    assert_eq!(live.deleted, false);

    let n = registry::delete(conn, &vec![sample_delete(&live)]).expect("delete");
    assert_eq!(n, 1);

    // Two rows for the same path: original (live) + appended tombstone.
    let rows: Vec<FileRecord> = file_records::table
        .filter(file_records::path.eq("a.cook"))
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load(conn)
        .unwrap();
    assert_eq!(rows.len(), 2, "delete is append-only; original row is preserved");
    assert_eq!(rows[0].deleted, false);
    assert_eq!(rows[1].deleted, true);
    assert!(rows[1].id > rows[0].id, "tombstone id must be newer");
}

#[test]
fn non_deleted_returns_latest_live_row_per_path_scoped_by_namespace() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    // Namespace 1: "a.cook" created, then re-created (modified), then namespace 2
    // has its own unrelated "a.cook".
    registry::create(
        conn,
        &vec![
            sample_create("a.cook", 10, 1), // id 1 (ns 1, old)
            sample_create("b.cook", 20, 1), // id 2 (ns 1)
        ],
    )
    .unwrap();

    // Modified-file path: append a new CreateForm with a larger size.
    let mut modified = sample_create("a.cook", 11, 1);
    modified.modified_at = OffsetDateTime::from_unix_timestamp(1_700_000_500).unwrap();
    registry::create(conn, &vec![modified]).unwrap(); // id 3 (ns 1, newer)

    // A deleted file in ns 1.
    registry::create(conn, &vec![sample_create("c.cook", 30, 1)]).unwrap(); // id 4
    let c: FileRecord = file_records::table
        .filter(file_records::path.eq("c.cook"))
        .select(FileRecord::as_select())
        .first(conn)
        .unwrap();
    registry::delete(conn, &vec![sample_delete(&c)]).unwrap(); // id 5 (tombstone)

    // Namespace 2 rows must not leak into namespace 1.
    registry::create(conn, &vec![sample_create("a.cook", 999, 2)]).unwrap(); // id 6

    let live = registry::non_deleted(conn, 1).expect("non_deleted ns 1");
    let paths: Vec<(&str, i64)> = live.iter().map(|r| (r.path.as_str(), r.size)).collect();
    // Expect: b.cook (id 2) then a.cook (id 3, latest live row). c.cook is hidden
    // (tombstone is latest). Order is by `id.asc()` per `non_deleted`'s SQL.
    assert_eq!(paths, vec![("b.cook", 20), ("a.cook", 11)]);

    let live_ns2 = registry::non_deleted(conn, 2).unwrap();
    assert_eq!(live_ns2.len(), 1);
    assert_eq!(live_ns2[0].path, "a.cook");
    assert_eq!(live_ns2[0].size, 999);
}

#[test]
fn non_deleted_empty_db_returns_empty_vec() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");
    let live = registry::non_deleted(conn, 1).unwrap();
    assert!(live.is_empty());
}
