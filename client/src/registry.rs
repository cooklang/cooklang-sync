use diesel::prelude::*;
use diesel::{insert_into, update};

use log::{trace};

use crate::models::*;
use crate::schema::*;
use crate::connection::Connection;

type Result<T, E = diesel::result::Error> = std::result::Result<T, E>;

pub fn create(conn: &mut Connection, forms: &Vec<CreateForm>) -> Result<usize> {
    trace!("inserting {:?}", forms);

    insert_into(file_records::table).values(forms).execute(conn)
}

pub fn update_jid(conn: &mut Connection, file_record: &FileRecord, jid: i32) -> Result<usize> {
    trace!("update_jid {:?}: {:?}", jid, file_record);

    update(file_records::table)
        .filter(file_records::id.eq(file_record.id))
        .set(file_records::jid.eq(jid))
        .execute(conn)
}

pub fn delete(conn: &mut Connection, ids: &Vec<i32>) -> Result<usize> {
    trace!("marking as deleted ids {:?}", ids);

    update(file_records::table)
        .filter(file_records::id.eq_any(ids))
        .set(file_records::deleted.eq(true))
        .execute(conn)
}

pub fn non_deleted(conn: &mut Connection) -> Result<Vec<FileRecord>> {
    trace!("non_deleted");

    file_records::table
        .filter(file_records::deleted.eq(false))
        .select(FileRecord::as_select())
        .order(file_records::id.desc())
        .load::<FileRecord>(conn)
}

/// Files that don't have jid and not deleted
/// These should be send to remote
pub fn updated_locally(conn: &mut Connection) -> Result<Vec<FileRecord>> {
    trace!("non_deleted");

    file_records::table
        .filter(file_records::deleted.eq(false))
        .filter(file_records::jid.is_null())
        .select(FileRecord::as_select())
        .order(file_records::id.desc())
        .load::<FileRecord>(conn)
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
        Err(e) => Err(e)
    }
}
