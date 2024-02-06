use crate::schema::file_records;
use diesel::prelude::*;
use rocket::serde::{Deserialize, Serialize};

#[derive(Queryable, Selectable, Identifiable, Deserialize, Serialize, Clone, Debug)]
#[diesel(table_name = file_records)]
#[serde(crate = "rocket::serde")]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecord {
    pub id: i32,
    pub path: String,
    pub deleted: bool,
    pub chunk_ids: String,
    pub format: String,
}

#[derive(Insertable, Deserialize, Serialize, Clone, Debug)]
#[diesel(table_name = file_records)]
#[serde(crate = "rocket::serde")]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct NewFileRecord {
    pub path: String,
    pub chunk_ids: String,
    pub deleted: bool,
    pub format: String,
}
