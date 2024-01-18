use diesel::sqlite::SqliteConnection;
use diesel::prelude::*;

use crate::models::*;

pub struct LocalDB {
    connection: SqliteConnection
}

impl LocalDB {

    pub fn new(database_url: String) -> LocalDB {
        let connection = SqliteConnection::establish(&database_url)
            .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

        LocalDB { connection }
    }

    fn query_file_record(&self) -> Vec<FileRecord>{
        todo!()
    }

    fn create_file_record(&self, create_form: &CreateForm) -> FileRecord {
        todo!()
    }

    fn update_file_record(&self, update_form: &UpdateForm) -> FileRecord {
        todo!()
    }

    fn latest_file_record(&self, path: String) -> FileRecord {
        todo!()
    }

    // no delete on purpose


}
