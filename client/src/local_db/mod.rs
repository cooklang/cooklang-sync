use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::{insert_into, update};

use diesel::r2d2::ConnectionManager;
use diesel::r2d2::Pool;

use log::{trace, error};

use crate::models::*;
use crate::schema::file_records::dsl::*;
use crate::errors::SyncError;

/// Append only registry of file records.

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

fn run_migrations(connection: &mut SqliteConnection) -> Result<(), SyncError> {

    if let Err(e) = connection.run_pending_migrations(MIGRATIONS) {
        error!("Error in run_migrations: {}", e);

        return Err(SyncError::RunMigrationError)
    }

    Ok(())
}

pub type ConnectionPool = Pool<ConnectionManager<SqliteConnection>>;

pub fn get_connection_pool(db_path: &str) -> ConnectionPool {
    let manager = ConnectionManager::<SqliteConnection>::new(db_path);

    let pool = Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build connection pool for DB.");

    let conn = &mut pool.get().unwrap();

    if let Err(e) = run_migrations(conn) {
        error!("Failed to run migrations");
    }

    pool
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

pub fn non_deleted_file_records(conn: &mut SqliteConnection) -> Vec<FileRecord> {
    trace!("non_deleted_file_records");
    file_records
        .filter(deleted.eq(false))
        .select(FileRecord::as_select())
        .order(id.desc())
        .load::<FileRecord>(conn)
        .unwrap()
}

/// Files that don't have jid and not deleted
/// These should be send to remote
pub fn updated_locally_file_records(conn: &mut SqliteConnection) -> Vec<FileRecord> {
    trace!("non_deleted_file_records");
    file_records
        .filter(deleted.eq(false))
        .filter(jid.is_null())
        .select(FileRecord::as_select())
        .order(id.desc())
        .load::<FileRecord>(conn)
        .unwrap()
}
