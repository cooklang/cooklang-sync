use crate::schema::file_records;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use diesel::{AsExpression, FromSqlRow};

use diesel::sql_types::Integer;

#[repr(i32)]
#[derive(Debug, Clone, Copy, AsExpression, FromSqlRow, PartialEq, Deserialize, Serialize)]
#[diesel(sql_type = Integer)]
pub enum FileFormat {
    Binary = 1,
    Text = 2,
}

#[derive(Debug)]
pub enum IndexerUpdateEvent {
    Updated,
}

#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecord {
    pub id: i32,
    pub jid: Option<i32>,
    pub deleted: bool,
    pub path: String,
    pub size: i64,
    pub modified_at: OffsetDateTime,
    pub namespace_id: i32,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct CreateForm {
    pub jid: Option<i32>,
    pub path: String,
    pub deleted: bool,
    pub size: i64,
    pub modified_at: OffsetDateTime,
    pub namespace_id: i32,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DeleteForm {
    pub path: String,
    pub jid: Option<i32>,
    pub size: i64,
    pub modified_at: OffsetDateTime,
    pub deleted: bool,
    pub namespace_id: i32,
}

#[derive(AsChangeset, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordUpdateForm {
    pub size: i64,
    pub modified_at: OffsetDateTime,
}

#[derive(AsChangeset, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordNonDeletedFilterForm {
    pub deleted: bool,
}

#[derive(AsChangeset, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordDeleteForm {
    pub id: i32,
    pub deleted: bool,
    pub namespace_id: i32,
}

impl PartialEq<CreateForm> for FileRecord {
    fn eq(&self, other: &CreateForm) -> bool {
        self.path == other.path && self.size == other.size && self.modified_at == other.modified_at
    }
}
