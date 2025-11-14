use diesel::prelude::*;
use rocket::serde::{Deserialize, Serialize};

use super::db::DieselBackend;
use super::schema::file_records;

#[derive(Queryable, Selectable, Identifiable, Deserialize, Serialize, Debug)]
#[diesel(table_name = file_records)]
#[serde(crate = "rocket::serde")]
#[diesel(check_for_backend(DieselBackend))]
pub struct FileRecord {
    pub id: i32,
    pub user_id: i32,
    pub chunk_ids: String,
    pub deleted: bool,
    pub path: String,
}

#[derive(Insertable, Deserialize, Serialize, Debug)]
#[diesel(table_name = file_records)]
#[serde(crate = "rocket::serde")]
#[diesel(check_for_backend(DieselBackend))]
pub struct NewFileRecord {
    pub user_id: i32,
    pub chunk_ids: String,
    pub deleted: bool,
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_file_record_construction() {
        let record = NewFileRecord {
            user_id: 123,
            chunk_ids: "hash1,hash2,hash3".to_string(),
            deleted: false,
            path: "recipes/test.cook".to_string(),
        };

        assert_eq!(record.user_id, 123);
        assert_eq!(record.chunk_ids, "hash1,hash2,hash3");
        assert_eq!(record.deleted, false);
        assert_eq!(record.path, "recipes/test.cook");
    }

    #[test]
    fn test_new_file_record_with_deleted_flag() {
        let record = NewFileRecord {
            user_id: 456,
            chunk_ids: "".to_string(),
            deleted: true,
            path: "recipes/deleted.cook".to_string(),
        };

        assert_eq!(record.deleted, true);
        assert_eq!(record.chunk_ids, "");
    }

    #[test]
    fn test_file_record_construction() {
        let record = FileRecord {
            id: 1,
            user_id: 123,
            chunk_ids: "hash1,hash2".to_string(),
            deleted: false,
            path: "test/path.cook".to_string(),
        };

        assert_eq!(record.id, 1);
        assert_eq!(record.user_id, 123);
        assert_eq!(record.chunk_ids, "hash1,hash2");
        assert_eq!(record.deleted, false);
        assert_eq!(record.path, "test/path.cook");
    }
}
