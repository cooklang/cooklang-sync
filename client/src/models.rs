use crate::schema::file_records;
use diesel::prelude::*;
use time::OffsetDateTime;

#[derive(Debug)]
pub enum IndexerUpdateEvent {
    Updated,
}

#[derive(Queryable, Selectable, Identifiable, Clone, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecord {
    pub id: i32,
    pub jid: Option<i32>,
    pub deleted: bool,
    pub path: String,
    pub format: String,
    pub size: i64,
    pub modified_at: OffsetDateTime,
}

#[derive(Insertable, Clone, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct CreateForm {
    pub jid: Option<i32>,
    pub path: String,
    pub format: String,
    pub deleted: bool,
    pub size: i64,
    pub modified_at: OffsetDateTime,
}

#[derive(Insertable, Clone, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DeleteForm {
    pub path: String,
    pub jid: Option<i32>,
    pub format: String,
    pub size: i64,
    pub modified_at: OffsetDateTime,
    pub deleted: bool,
}

#[derive(AsChangeset, Clone, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordUpdateForm {
    pub size: i64,
    pub modified_at: OffsetDateTime,
}

#[derive(AsChangeset, Clone, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordNonDeletedFilterForm {
    pub deleted: bool,
}

#[derive(AsChangeset, Clone, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordDeleteForm {
    pub id: i32,
    pub deleted: bool,
}

impl PartialEq<CreateForm> for FileRecord {
    fn eq(&self, other: &CreateForm) -> bool {
        self.path == other.path
            && self.format == other.format
            && self.size == other.size
            && self.modified_at == other.modified_at
    }
}
