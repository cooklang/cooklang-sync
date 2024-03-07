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

pub fn non_deleted(conn: &mut Connection) -> Result<Vec<FileRecord>> {
    trace!("non_deleted");

    // Consider only latest record for the same path.
    let subquery = file_records::table
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
pub fn updated_locally(conn: &mut Connection) -> Result<Vec<FileRecord>> {
    trace!("updated_locally");

    // Need to ignore records which come after record with jid
    // for the same path
    let subquery = file_records::table
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

pub fn latest_jid(conn: &mut Connection) -> Result<i32> {
    trace!("latest_jid");

    let r = file_records::table
        .filter(file_records::jid.is_not_null())
        .select(FileRecord::as_select())
        .order(file_records::jid.desc())
        .first::<FileRecord>(conn);

    match r {
        Ok(r) => Ok(r.jid.unwrap_or(0)),
        Err(e) => Err(e),
    }
}
