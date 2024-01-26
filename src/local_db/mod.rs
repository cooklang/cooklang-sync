use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::{insert_into, update};

use diesel::r2d2::ConnectionManager;
use diesel::r2d2::Pool;

use log::trace;

use crate::models::*;
use crate::schema::file_records::dsl::*;

/// Append only registry of file records.

pub type ConnectionPool = Pool<ConnectionManager<SqliteConnection>>;

pub fn get_connection_pool(db_path: &str) -> ConnectionPool {
    let manager = ConnectionManager::<SqliteConnection>::new(db_path);

    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build connection pool for DB.")
}

pub fn create_file_records(
    conn: &mut SqliteConnection,
    create_forms: &Vec<FileRecordCreateForm>,
) -> Result<usize, diesel::result::Error> {
    trace!("inserting {:?}", create_forms);
    insert_into(file_records).values(create_forms).execute(conn)
}

pub fn delete_file_records(
    conn: &mut SqliteConnection,
    ids: &Vec<i32>,
) -> Result<usize, diesel::result::Error> {
    trace!("marking as deleted ids {:?}", ids);
    update(file_records)
        .filter(id.eq_any(ids))
        .set(deleted.eq(true))
        .execute(conn)
}

pub fn latest_file_records(
    conn: &mut SqliteConnection,
    filter_form: &FileRecordNonDeletedFilterForm,
) -> Vec<FileRecord> {
    trace!("latest_file_records");
    file_records
        .filter(deleted.eq(filter_form.deleted))
        .select(FileRecord::as_select())
        .order(id.desc())
        .load::<FileRecord>(conn)
        .unwrap()
}
