use crate::schema::file_records;
use diesel::prelude::*;
use time::OffsetDateTime;

#[derive(Queryable, Selectable, Identifiable, AsChangeset, Clone, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecord {
    pub id: i32,
    pub jid: Option<i32>,
    pub path: String,
    pub format: String,
    pub size: i64,
    pub modified_at: OffsetDateTime,
}

#[derive(Insertable, Clone, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordCreateForm {
    pub path: String,
    pub format: String,
    pub size: i64,
    pub modified_at: OffsetDateTime,
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
pub struct FileRecordFilterForm<'a> {
    pub path: &'a str,
}

impl PartialEq<FileRecordCreateForm> for FileRecord {
    fn eq(&self, other: &FileRecordCreateForm) -> bool {
        self.path == other.path
            && self.format == other.format
            && self.size == other.size
            && self.modified_at == other.modified_at
    }
}
