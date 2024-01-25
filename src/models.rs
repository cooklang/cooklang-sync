use crate::schema::file_records;
use diesel::prelude::*;
use time::OffsetDateTime;

#[derive(Queryable, Selectable, Identifiable, AsChangeset, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecord {
    pub id: i32,
    pub jid: Option<i32>,
    pub path: String,
    pub format: String,
    pub size: Option<i64>,
    pub modified_at: Option<OffsetDateTime>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordCreateForm<'a> {
    pub path: &'a str,
    pub format: &'a str,
    pub size: Option<i64>,
    pub modified_at: Option<OffsetDateTime>,
}

#[derive(AsChangeset, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordUpdateForm {
    pub size: Option<i64>,
    pub modified_at: Option<OffsetDateTime>,
}

#[derive(AsChangeset, Debug)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordFilterForm<'a> {
    pub path: &'a str,
}

impl PartialEq<FileRecordCreateForm<'_>> for FileRecord {
    fn eq(&self, other: &FileRecordCreateForm) -> bool {
        self.path == other.path
            && self.format == other.format
            && self.size == other.size
            && self.modified_at == other.modified_at
    }
}
