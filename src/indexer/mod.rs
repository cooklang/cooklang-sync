

// pub struct Indexer {
//     db: LocalDB
// }

// enum FileCheckResult {
//     Matched,
//     NotMatched
// }

// impl Indexer {

//     fn run() {
//         check_all_files();

//         // on rx that file updated:
//         // check file
//         // callback(result)
//         // emit ready to be synced ready_to_updoad_tx
//     }

//     // should be done on app start and on regular intervals after
//     fn check_all_files(&self) {
//         // all_files().each {
//         //     let result = check_file(path);
//         //     callback(result)
//         // }
//     }

//     fn callback(result: FileCheckResult) {
//         // match result {
//         //     Matched => nothing to do,
//         //     NotMatched => store in db without jid, tell that local_file_updated
//         // }
//     }


//     fn check_file(&self, path: String) {
//         // query from db
//         // file stored
//         // compare_with_db

//         // returns FileCheckResult
//     }

//     fn file_stored(&self, path) -> FileOnDisk {
//         // get file metadata
//     }

//     fn compare_with_db(file_stored, db_record) {
//         // compare metadata, size
//     }

// }
