// mod remote;

// pub struct Syncer {

// }

// impl Syncer {

//     pub fn new() -> Syncer {
//         Syncer {}
//     }

//     fn run_upload() {
//         // sync_with_remote in intervals
//     }

//     fn run_download() {
//         // watch on local updates and then local_file_updated
//     }

//     //
//     // upload changes logic
//     //

//     // should be called after indexer figures out that local file was changed
//     // if local file was updated but app just started? - maybe indexer will check modified time stored in DB
//     // and find ones which are different from stored in DB
//     fn local_file_updated(&self, path: String) {
//         // match commit() {
//         //     NeedUpload(hashes) => {
//         //         while {
//         //             store_batch // optimised by content size
//         //         }

//         //         local_file_updated()
//         //     }
//         //     Commited => store new jid to local DB
//         //     Synced =>
//         //     Error =>
//         // }
//     }

//     // content should be data bytes instead
//     fn commit(&self, namespace: i64, path: String, chunks: Vec<String>) {
//         // will try to upload list of chunks and get a new jid for path
//         // it can get two options in response:
//         //   - list of chunks that's missing
//         //   - success message with new jid
//         // can also fail
//     }

//     fn store_batch(&self, chunks: Vec<String>, content: Vec<String>) {
//         // upload content of chunks to remote
//         // one option in response:
//         //   - success
//         // can also fail
//     }

//     fn is_synced(&self, namespace: i64, path: String, jid: i64) {
//         // query latest remote jid and compare with local one
//         // two options can be in response:
//         //   - we're in sync
//         //   - we're behind
//         // can also fail
//     }

//     //
//     // download changes logic
//     //

//     // should be called on cron or on long-living request
//     fn sync_with_remote(&self) {
//         // let jid = localDB.latestJid
//         // let updates = list(jid)

//         // let chunks_to_retrieve: Vec<String>
//         // updates.each {
//         //     update.each_chunk {
//         //         chunks_to_retrieve << chunk unless chunker.contains
//         //     }

//         // }

//         // chunks_to_retrieve.each {
//         //     retrieve_batch
//         // }

//         // updates.each. {
//         //     store to db filepath + jid
//         // }
//     }

//     fn list(&self, namespace: i64, jid: i64) {
//         // will return list of all changes since jid
//         // one response:
//         //   - an array of changes for each file [(ns_id: 1, jid: 15, "/breakfast/Burrito.cook", "chunk1,chunk2")] (only latest for a specific file)
//     }

//     fn retrieve_batch(&self, chunks: Vec<String>, content: Vec<String>) {
//         // download content of chunks from remote
//         // one option in response:
//         //   - success
//         // can also fail
//     }

// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_local_file_updated() {
//         let syncer = Syncer::new();
//         syncer.local_file_updated("path/to/file".to_string());
//         // Add assertions and checks as necessary
//     }

//     #[test]
//     fn test_commit() {
//         let syncer = Syncer::new();
//         syncer.commit(1, "path/to/file".to_string(), vec!["chunk1".to_string()]);
//         // Add assertions and checks as necessary
//     }

//     #[test]
//     fn test_store_batch() {
//         let syncer = Syncer::new();
//         syncer.store_batch(vec!["chunk1".to_string()], vec!["content1".to_string()]);
//         // Add assertions and checks as necessary
//     }

//     #[test]
//     fn test_is_synced() {
//         let syncer = Syncer::new();
//         syncer.is_synced(1, "path/to/file".to_string(), 42);
//         // Add assertions and checks as necessary
//     }

//     #[test]
//     fn test_sync_with_remote() {
//         let syncer = Syncer::new();
//         syncer.sync_with_remote();
//         // Add assertions and checks as necessary
//     }

//     #[test]
//     fn test_list() {
//         let syncer = Syncer::new();
//         syncer.list(1, 42);
//         // Add assertions and checks as necessary
//     }

//     #[test]
//     fn test_retrieve_batch() {
//         let syncer = Syncer::new();
//         syncer.retrieve_batch(vec!["chunk1".to_string()], vec!["content1".to_string()]);
//         // Add assertions and checks as necessary
//     }
// }
