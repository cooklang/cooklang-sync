use crate::schema::file_records;
use diesel::prelude::*;
use rocket::serde::{Deserialize, Serialize};

#[derive(Queryable, Selectable, Identifiable, Deserialize, Serialize, Debug)]
#[diesel(table_name = file_records)]
#[serde(crate = "rocket::serde")]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecord {
    pub id: i32,
    pub chunk_ids: String,
    pub deleted: bool,
    pub format: String,
    pub path: String,
}

#[derive(Insertable, Deserialize, Serialize, Debug)]
#[diesel(table_name = file_records)]
#[serde(crate = "rocket::serde")]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct NewFileRecord {
    pub chunk_ids: String,
    pub deleted: bool,
    pub format: String,
    pub path: String,
}
