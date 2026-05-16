use diesel::dsl::{max, sql};
use diesel::prelude::*;
use diesel::{insert_into, update};

use log::trace;

use crate::connection::Connection;
use crate::models::*;
use crate::schema::*;

type Result<T, E = diesel::result::Error> = std::result::Result<T, E>;

pub fn create(conn: &mut Connection, forms: &Vec<CreateForm>) -> Result<usize> {
    trace!("inserting {:?}", forms);

    insert_into(file_records::table).values(forms).execute(conn)
}

pub fn update_jid(conn: &mut Connection, record: &FileRecord, jid: i32) -> Result<usize> {
    trace!("update_jid {:?}: {:?}", jid, record);

    update(file_records::table)
        .filter(file_records::id.eq(record.id))
        .set(file_records::jid.eq(jid))
        .execute(conn)
}

pub fn delete(conn: &mut Connection, forms: &Vec<DeleteForm>) -> Result<usize> {
    trace!("marking as deleted {:?}", forms);

    insert_into(file_records::table).values(forms).execute(conn)
}

pub fn non_deleted(conn: &mut Connection, namespace_id: i32) -> Result<Vec<FileRecord>> {
    trace!("non_deleted");

    // Consider only latest record for the same path.
    let subquery = file_records::table
        .filter(file_records::namespace_id.eq(namespace_id))
        .group_by(file_records::path)
        .select(max(file_records::id))
        .into_boxed()
        .select(sql::<diesel::sql_types::Integer>("max(id)"));

    file_records::table
        .filter(file_records::deleted.eq(false))
        .filter(file_records::id.eq_any(subquery))
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load::<FileRecord>(conn)
}

/// Files that don't have jid
/// These should be send to remote
pub fn updated_locally(conn: &mut Connection, namespace_id: i32) -> Result<Vec<FileRecord>> {
    trace!("updated_locally");

    // Need to ignore records which come after record with jid
    // for the same path
    let subquery = file_records::table
        .filter(file_records::namespace_id.eq(namespace_id))
        .group_by(file_records::path)
        .select(max(file_records::id))
        .into_boxed()
        .select(sql::<diesel::sql_types::Integer>("max(id)"));

    let query = file_records::table
        .select(FileRecord::as_select())
        .filter(file_records::jid.is_null())
        .filter(file_records::id.eq_any(subquery))
        .order(file_records::id.asc());

    query.load::<FileRecord>(conn)
}

pub fn latest_jid(conn: &mut Connection, namespace_id: i32) -> Result<i32> {
    trace!("latest_jid");

    let r = file_records::table
        .filter(file_records::jid.is_not_null())
        .filter(file_records::namespace_id.eq(namespace_id))
        .select(FileRecord::as_select())
        .order(file_records::jid.desc())
        .first::<FileRecord>(conn);

    match r {
        Ok(r) => Ok(r.jid.unwrap_or(0)),
        Err(e) => Err(e),
    }
}

/// Read the incremental-download watermark for a namespace.
///
/// The watermark is advanced atomically only after a full download batch
/// completes (see `syncer::check_download_once`). This prevents the bug
/// that arises when a per-file advancing watermark (e.g. `max(jid)` across
/// `file_records`) drifts past lower-jid files that the client hasn't yet
/// persisted — the server's `list(jid)` filter then hides those files
/// forever.
///
/// Returns 0 if no row exists for `namespace_id`, which triggers a full
/// initial sync — the safe default on first use and after a schema upgrade.
pub fn get_download_watermark(conn: &mut Connection, namespace_id: i32) -> Result<i32> {
    trace!("get_download_watermark ns={}", namespace_id);

    let r = sync_state::table
        .filter(sync_state::namespace_id.eq(namespace_id))
        .select(sync_state::download_watermark)
        .first::<i32>(conn)
        .optional()?;

    Ok(r.unwrap_or(0))
}

/// Advance the incremental-download watermark for a namespace.
///
/// Callers must only invoke this after every file in the current batch has
/// been persisted to the local registry; otherwise a subsequent incremental
/// request will skip still-missing files (see `get_download_watermark`).
pub fn set_download_watermark(
    conn: &mut Connection,
    namespace_id: i32,
    value: i32,
) -> Result<usize> {
    trace!("set_download_watermark ns={} value={}", namespace_id, value);

    // Upsert: SQLite ON CONFLICT on the PRIMARY KEY updates the row in place.
    insert_into(sync_state::table)
        .values((
            sync_state::namespace_id.eq(namespace_id),
            sync_state::download_watermark.eq(value),
        ))
        .on_conflict(sync_state::namespace_id)
        .do_update()
        .set(sync_state::download_watermark.eq(value))
        .execute(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{get_connection, get_connection_pool};
    use tempfile::TempDir;

    fn setup() -> (TempDir, crate::connection::ConnectionPool) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = get_connection_pool(db_path.to_str().unwrap()).unwrap();
        (tmp, pool)
    }

    #[test]
    fn given_no_row_when_get_download_watermark_then_returns_zero() {
        // Given
        let (_tmp, pool) = setup();
        let mut conn = get_connection(&pool).unwrap();

        // When
        let watermark = get_download_watermark(&mut conn, 1).unwrap();

        // Then
        assert_eq!(watermark, 0);
    }

    #[test]
    fn given_set_watermark_when_get_download_watermark_then_returns_stored_value() {
        // Given
        let (_tmp, pool) = setup();
        let mut conn = get_connection(&pool).unwrap();
        set_download_watermark(&mut conn, 1, 42).unwrap();

        // When
        let watermark = get_download_watermark(&mut conn, 1).unwrap();

        // Then
        assert_eq!(watermark, 42);
    }

    #[test]
    fn given_existing_value_when_set_download_watermark_then_overwrites_without_duplicating() {
        // Given
        let (_tmp, pool) = setup();
        let mut conn = get_connection(&pool).unwrap();
        set_download_watermark(&mut conn, 1, 100).unwrap();

        // When
        set_download_watermark(&mut conn, 1, 200).unwrap();

        // Then
        assert_eq!(get_download_watermark(&mut conn, 1).unwrap(), 200);
        let row_count: i64 = sync_state::table
            .filter(sync_state::namespace_id.eq(1))
            .count()
            .get_result(&mut *conn)
            .unwrap();
        assert_eq!(row_count, 1);
    }

    #[test]
    fn given_different_namespaces_when_set_download_watermark_then_values_isolated() {
        // Given
        let (_tmp, pool) = setup();
        let mut conn = get_connection(&pool).unwrap();

        // When
        set_download_watermark(&mut conn, 1, 100).unwrap();
        set_download_watermark(&mut conn, 2, 200).unwrap();

        // Then
        assert_eq!(get_download_watermark(&mut conn, 1).unwrap(), 100);
        assert_eq!(get_download_watermark(&mut conn, 2).unwrap(), 200);
    }
}
