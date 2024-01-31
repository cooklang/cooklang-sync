use crate::schema::file_records;
use diesel::prelude::*;
use rocket::serde::{Serialize, Deserialize, json::Json};


#[derive(Queryable, Selectable, Identifiable, Deserialize, Serialize, Clone, Debug)]
#[diesel(table_name = file_records)]
#[serde(crate = "rocket::serde")]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecord {
    pub id: i32,
    pub deleted: bool,
    pub path: String,
    pub format: String,
}
