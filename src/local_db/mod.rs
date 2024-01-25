use diesel::insert_into;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::OptionalExtension;

use diesel::r2d2::ConnectionManager;
use diesel::r2d2::Pool;

use log::{debug};

use crate::models::*;
use crate::schema::file_records::dsl::*;

pub fn get_connection_pool(db_path: &str) -> Pool<ConnectionManager<SqliteConnection>> {
    let manager = ConnectionManager::<SqliteConnection>::new(db_path);

    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build connection pool for DB.")
}

pub fn create_file_record(
    conn: &mut SqliteConnection,
    create_form: &FileRecordCreateForm,
) -> Result<usize, diesel::result::Error> {
    debug!("inserting {:?} into DB", create_form);
    insert_into(file_records).values(create_form).execute(conn)
}

// pub fn update_file_record(
//     _conn: &mut SqliteConnection,
//     _update_form: FileRecordUpdateForm,
// ) -> FileRecord {
//     todo!()
// }

// TODO: use filter form, not only path
pub fn latest_file_record(
    conn: &mut SqliteConnection,
    filter_form: &FileRecordFilterForm,
) -> Option<FileRecord> {
    file_records
        .filter(path.eq(filter_form.path))
        .select(FileRecord::as_select())
        .order(id.desc())
        .first::<FileRecord>(conn)
        .optional()
        .unwrap()
}
