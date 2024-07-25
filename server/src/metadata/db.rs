use diesel::dsl::{max, sql};
use diesel::prelude::*;
use rocket_sync_db_pools::database;

use super::models::*;
use super::schema::*;

#[cfg(feature = "database_sqlite")]
type DbConnection = diesel::SqliteConnection;

#[cfg(feature = "database_postgres")]
type DbConnection = diesel::PgConnection;


#[database("diesel")]
pub(crate) struct Db(DbConnection);

#[cfg(all(feature = "database_postgres"))]
#[allow(dead_code)]
pub type DieselBackend = diesel::pg::Pg;

#[cfg(all(feature = "database_sqlite"))]
#[allow(dead_code)]
pub type DieselBackend = diesel::sqlite::Sqlite;


type Result<T, E = diesel::result::Error> = std::result::Result<T, E>;

pub fn insert_new_record(conn: &mut DbConnection, record: NewFileRecord) -> Result<i32> {
    diesel::insert_into(file_records::table)
        .values(record)
        .returning(file_records::id)
        .get_result(conn)
}

pub fn list(conn: &mut DbConnection, user_id: i32, jid: i32) -> Result<Vec<FileRecord>> {
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
