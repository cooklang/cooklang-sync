use diesel::dsl::{max, sql};
use diesel::prelude::*;
use rocket_sync_db_pools::database;

use crate::models::*;
use crate::schema::*;

#[database("diesel")]
pub(crate) struct Db(diesel::SqliteConnection);

type Result<T, E = diesel::result::Error> = std::result::Result<T, E>;

pub fn insert_new_record(conn: &mut SqliteConnection, record: NewFileRecord) -> Result<i32> {
    diesel::insert_into(file_records::table)
        .values(record)
        .returning(file_records::id)
        .get_result(conn)
}

pub fn list(conn: &mut SqliteConnection, user_id: i32, jid: i32) -> Result<Vec<FileRecord>> {
    let subquery = file_records::table
        .filter(file_records::user_id.eq(user_id))
        .group_by(file_records::path)
        .select(max(file_records::id))
        .into_boxed()
        .select(sql::<diesel::sql_types::Integer>("max(id)"));

    file_records::table
        .filter(file_records::id.gt(jid))
        .filter(file_records::id.eq_any(subquery))
        .select(FileRecord::as_select())
        .load(conn)
}
