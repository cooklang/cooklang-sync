use diesel::insert_into;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::OptionalExtension;

use diesel::r2d2::ConnectionManager;
use diesel::r2d2::Pool;

use crate::models::*;
use crate::schema::file_records::dsl::*;

pub fn get_connection_pool(db_path: &str) -> Pool<ConnectionManager<SqliteConnection>> {
    let manager = ConnectionManager::<SqliteConnection>::new(db_path);
    // Refer to the `r2d2` documentation for more methods to use
    // when building a connection pool
    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build connection pool")
}

pub fn query_file_record(_conn: &mut SqliteConnection) -> Vec<FileRecord> {
    todo!()
}

pub fn create_file_record(
    conn: &mut SqliteConnection,
    create_form: FileRecordCreateForm,
) -> Result<usize, diesel::result::Error> {
    println!("inserting into {:?}", create_form);
    insert_into(file_records).values(create_form).execute(conn)
}

pub fn update_file_record(
    _conn: &mut SqliteConnection,
    _update_form: FileRecordUpdateForm,
) -> FileRecord {
    todo!()
}

pub fn latest_file_record(conn: &mut SqliteConnection, file_path: String) -> Option<FileRecord> {
    file_records
        .filter(path.eq(file_path))
        .select(FileRecord::as_select())
        .order(id.desc())
        .first::<FileRecord>(conn)
        .optional()
        .unwrap()
}
