use diesel::prelude::*;
use crate::schema::*;

#[derive(Queryable, Selectable, Identifiable)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecord {
    pub id: i32,
    pub path: String,
    pub format: String,
    pub size: Option<i32>,
    pub modified_at: Option<String>,
    pub created_at: String,
}


#[derive(AsChangeset)]
#[diesel(table_name = file_records)]
pub struct CreateForm<'a> {
    pub path: &'a str,
    pub format: &'a str,
    pub size: Option<i32>,
    pub modified_at: Option<&'a str>,
    pub created_at: &'a str,
}

#[derive(AsChangeset)]
#[diesel(table_name = file_records)]
pub struct UpdateForm<'a> {
    pub size: Option<i32>,
    pub modified_at: Option<&'a str>,
}
