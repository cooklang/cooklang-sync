
REMOTE SCHEMA

Server File Journal - stores all changes 
===================
Namespace Id (NSID)
Relative Path in namespace
Journal ID (JID): Monotonically increasing within a namespace


BlockServer - can store block or retrieve block
===========
- [ ] RocksDB might work

Q:
- where to store chunks? s3 is to expensive for such small files, maybe cheap distributed key/value db?




LOCAL DB SCHEMA
===============

files
-----
jid: integer
path // relative to current dir
format: text|binary
modified: unix timestamp
size: integer
is_symlink: bool
checksum: varchar


USE-CASES
=========
- client needs to update a file from meta server (MS)
    - S during polling receives that file /path/bla was updated
    - sends list request passing namespace and current cursor
    - MS returns all JIDs since passed one and their hashes (maybe except when the same file was updated multiple times, returns only the last one?)
    - S 
- client needs to upload a file to server
    - S tries to commit current file it has commit(/path/bla, [h1,h2,h3])
    - MS returns back list of 
- program just starts
    - S checks the latest journal_id
    - if local latest journal_id is the same it will do nothing
    - if local latest journal_id
- file was removed locally
- file was moved locally
- file was renamed
- one line in a file was edited
- one line in a file was added
- one line in a file was removed


if latest jid remotely bigger sync dowload from remote
if metadata, size is different upload to remote and after commit store into local db


Q:
- do I need hierarchy of services or they should be all independent?
- how sharing should work?
- how to thread it? multiple modules and multiple files
- do I need to sync file metadata as well?


> We have separate threads for sniffing the file system, hashing, commit, store_batch, list, retrieve_batch, and reconstruct, allowing us to pipeline parallelize this process across many files. We use compression and rsync to minimize the size of store_batch/retrieve_batch requests.


SYNCER
======
- [ ] checks if database has not assigned jid
- [ ] when it finds not assigned jid it will try to commit, after commiting it will update local DB with new jid
- [ ] if chunk is not present locally it will try to download it
- [ ] if chunk is not present remotely it will try to upload it

commit("breakfast/Mexican Style Burrito.cook", "h1,h2,h3");

Q:
- problem if by line? => seek wont work, need to store block size to do the seek effeftively.
- where to store chunks for not yet assembled file
- how to understand that a new file created remotely
- hot to understand that file was deleted
- how to understand that


INDEXER
=======
- [ ] sync between files and local DB on schedule (once a min, f.e.)
- [ ] watches changes and triggers sync
- [ ] will cleanup DB once a day

Q:
- do I need to copy not changed jid? or just update updated? => it makes sense to update all
- what happens on delete, move?


CHUNKER
=======

Role of Chunker is to deal with persistance of hashes and files. It operates on text files and chunks are not a fixed sized but each chunk is a line of file.

- [ ] given path it will produce list of hashes of the file: `fn hashify(file_path: String) -> io::Result<Vec<String>>`
- [ ] given path and list of hashes it will save a new version of a file `fn save(file_path: String, Vec<String>) -> io::Result`. It should raise an error if cache doesn't have content for a specific chunk hash
- [ ] can read content of a specific chunk from cache  `fn read_chunk(chunk: String) -> io::Result<String>`
- [ ] can write content of a spefic chunk to cache  `fn save_chunk(chunk: String, content: String) -> io::Result`
- [ ] given two vectors of hashes it can compare them if they are the same  `fn compare_sets(left: Vec<String>, right: Vec<String>) -> bool`
- [ ] given hash it can check if cache contains content for it or not.  `fn check_chunk(chunk: String>) -> io::Result<bool>`

Q:
- strings will be short, 80-100 symbols. what should be used as hashing function? what size of hash should be? I'd say square root of 10. You can test it!

- empty files should be different from deleted



TODO
====
- bundling of uploads/downloads
- uniffi bindings
- proper error handling
- can be issue that chunk is not in cache when we try to write file
- report error on unexpeted cache behaviour
- auth
- read-only
- namespaces
- add documentation
- extract to core shared datasctuctures
- filter stream events to propagate only required files
- support of binary files
    - by extension select text files, the rest is binary
- test test test
- namespace mounting
- metrics for monitoring (cache saturation, miss)
- draw data-flow
- garbage collection
- auto-update
- make sure non-blocking behavour
- make it work whenb offline
