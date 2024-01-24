use diesel::insert_into;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::OptionalExtension;

use crate::models::*;
use crate::schema::file_records::dsl::*;

pub struct LocalDB {
    connection: SqliteConnection,
}

impl LocalDB {
    pub fn new(database_url: &str) -> LocalDB {
        let connection = SqliteConnection::establish(database_url)
            .unwrap_or_else(|_| panic!("Error connecting to database"));

        LocalDB { connection }
    }

    pub fn query_file_record(&mut self) -> Vec<FileRecord> {
        todo!()
    }

    pub fn create_file_record(
        &mut self,
        create_form: FileRecordCreateForm,
    ) -> Result<usize, diesel::result::Error> {
        println!("inserting into {:?}", create_form);
        insert_into(file_records)
            .values(create_form)
            .execute(&mut self.connection)
    }

    pub fn update_file_record(&mut self, _update_form: FileRecordUpdateForm) -> FileRecord {
        todo!()
    }

    pub fn latest_file_record(&mut self, file_path: String) -> Option<FileRecord> {
        file_records
            .filter(path.eq(file_path))
            .select(FileRecord::as_select())
            .order(id.desc())
            .first::<FileRecord>(&mut self.connection)
            .optional()
            .unwrap()
    }

    // no delete on purpose
}
