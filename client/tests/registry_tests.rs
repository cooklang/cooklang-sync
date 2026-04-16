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
