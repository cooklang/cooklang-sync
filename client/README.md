
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

## Usage (iOS/Swift)

### Installation

Add the package to your Xcode project:

```swift
dependencies: [
    .package(url: "https://github.com/cooklang/cooklang-sync.git", from: "0.3.0")
]
```

### Basic Example

```swift
import CooklangSyncClient
import Foundation

// 1. Create a status listener to receive sync updates
class MySyncListener: SyncStatusListener {
    func onStatusChanged(status: SyncStatus) {
        print("Sync status: \(status)")
    }

    func onComplete(success: Bool, message: String?) {
        print("Sync completed. Success: \(success), Message: \(message ?? "none")")
    }
}

// 2. Set up sync context
let context = SyncContext()
let listener = MySyncListener()
context.setListener(listener: listener)

// 3. Configure sync parameters
let storageDir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    .appendingPathComponent("recipes")
    .path
let dbFilePath = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    .appendingPathComponent("sync.db")
    .path
let apiEndpoint = "https://api.cooklang.org"
let remoteToken = "your_jwt_token_here"
let namespaceId: Int32 = 1

// 4. Run continuous sync (watches for file changes and syncs automatically)
Task {
    do {
        try run(
            context: context,
            storageDir: storageDir,
            dbFilePath: dbFilePath,
            apiEndpoint: apiEndpoint,
            remoteToken: remoteToken,
            namespaceId: namespaceId,
            downloadOnly: false  // Set to true for read-only sync
        )
    } catch {
        print("Sync error: \(error)")
    }
}

// 5. Cancel sync when needed
context.cancel()
```

### One-Time Sync Operations

For manual control over sync operations:

```swift
// Download updates from server (one-time)
Task {
    do {
        try runDownloadOnce(
            storageDir: storageDir,
            dbFilePath: dbFilePath,
            apiEndpoint: apiEndpoint,
            remoteToken: remoteToken,
            namespaceId: namespaceId
        )
        print("Download completed")
    } catch {
        print("Download error: \(error)")
    }
}

// Upload local changes to server (one-time)
Task {
    do {
        try runUploadOnce(
            storageDir: storageDir,
            dbFilePath: dbFilePath,
            apiEndpoint: apiEndpoint,
            remoteToken: remoteToken,
            namespaceId: namespaceId
        )
        print("Upload completed")
    } catch {
        print("Upload error: \(error)")
    }
}
```

### Advanced: Wait for Remote Updates

Use this to implement efficient background sync with server-sent events:

```swift
// Wait for remote updates, then download
Task {
    do {
        // This blocks until server notifies of changes or timeout
        try waitRemoteUpdate(apiEndpoint: apiEndpoint, remoteToken: remoteToken)

        // Now download the updates
        try runDownloadOnce(
            storageDir: storageDir,
            dbFilePath: dbFilePath,
            apiEndpoint: apiEndpoint,
            remoteToken: remoteToken,
            namespaceId: namespaceId
        )
    } catch {
        print("Error: \(error)")
    }
}
```

### Sync Status Values

The `SyncStatus` enum includes:
- `.idle` - Not syncing
- `.syncing` - Sync operation in progress
- `.uploading` - Currently uploading to server
- `.downloading` - Currently downloading from server
- `.error` - Sync encountered an error

### Best Practices

1. **Database Location**: Store the database in Application Support directory to persist across app updates
2. **Storage Directory**: Use a dedicated subdirectory in Documents for synced files
3. **Background Sync**: On iOS, use Background Tasks API to run periodic syncs
4. **Error Handling**: Always handle `SyncError` exceptions and notify users appropriately
5. **Cancellation**: Call `context.cancel()` before app termination to clean up resources
6. **Read-Only Mode**: Set `downloadOnly: true` if you want to prevent local changes from syncing to server

## Usage (Android/Kotlin)

### Installation

Add the JNA dependency and the native library to your Android project:

```kotlin
// In build.gradle.kts
dependencies {
    implementation("net.java.dev.jna:jna:5.13.0@aar")
    // Add the generated Kotlin bindings and native library
}
```

Build the Android library using the Rust toolchain and include it in your project's `jniLibs` directory.

### Basic Example

```kotlin
import org.cooklang.sync.*
import kotlinx.coroutines.*

// 1. Create a status listener to receive sync updates
class MySyncListener : SyncStatusListener {
    override fun onStatusChanged(status: SyncStatus) {
        when (status) {
            is SyncStatus.Idle -> println("Sync idle")
            is SyncStatus.Syncing -> println("Syncing...")
            is SyncStatus.Indexing -> println("Indexing files...")
            is SyncStatus.Downloading -> println("Downloading from server...")
            is SyncStatus.Uploading -> println("Uploading to server...")
            is SyncStatus.Error -> println("Error: ${status.message}")
        }
    }

    override fun onComplete(success: Boolean, message: String?) {
        println("Sync completed. Success: $success, Message: ${message ?: "none"}")
    }
}

// 2. Set up sync context
val context = SyncContext()
val listener = MySyncListener()
context.setListener(listener)

// 3. Configure sync parameters
val storageDir = context.getExternalFilesDir(null)?.resolve("recipes")?.absolutePath
    ?: throw IllegalStateException("Cannot access external files directory")
val dbFilePath = context.getDatabasePath("sync.db").absolutePath
val apiEndpoint = "https://api.cooklang.org"
val remoteToken = "your_jwt_token_here"
val namespaceId = 1

// 4. Run continuous sync in a coroutine (watches for file changes)
lifecycleScope.launch(Dispatchers.IO) {
    try {
        run(
            context = context,
            storageDir = storageDir,
            dbFilePath = dbFilePath,
            apiEndpoint = apiEndpoint,
            remoteToken = remoteToken,
            namespaceId = namespaceId,
            downloadOnly = false  // Set to true for read-only sync
        )
    } catch (e: SyncException) {
        Log.e("Sync", "Sync error", e)
    }
}

// 5. Cancel sync when needed
override fun onDestroy() {
    super.onDestroy()
    context.cancel()
}
```

### One-Time Sync Operations

For manual control over sync operations:

```kotlin
// Download updates from server (one-time)
lifecycleScope.launch(Dispatchers.IO) {
    try {
        runDownloadOnce(
            storageDir = storageDir,
            dbFilePath = dbFilePath,
            apiEndpoint = apiEndpoint,
            remoteToken = remoteToken,
            namespaceId = namespaceId
        )
        Log.d("Sync", "Download completed")
    } catch (e: SyncException) {
        Log.e("Sync", "Download error", e)
    }
}

// Upload local changes to server (one-time)
lifecycleScope.launch(Dispatchers.IO) {
    try {
        runUploadOnce(
            storageDir = storageDir,
            dbFilePath = dbFilePath,
            apiEndpoint = apiEndpoint,
            remoteToken = remoteToken,
            namespaceId = namespaceId
        )
        Log.d("Sync", "Upload completed")
    } catch (e: SyncException) {
        Log.e("Sync", "Upload error", e)
    }
}
```

### Advanced: Wait for Remote Updates

Use this to implement efficient background sync with server-sent events:

```kotlin
// Wait for remote updates, then download
lifecycleScope.launch(Dispatchers.IO) {
    try {
        // This blocks until server notifies of changes or timeout
        waitRemoteUpdate(apiEndpoint = apiEndpoint, remoteToken = remoteToken)

        // Now download the updates
        runDownloadOnce(
            storageDir = storageDir,
            dbFilePath = dbFilePath,
            apiEndpoint = apiEndpoint,
            remoteToken = remoteToken,
            namespaceId = namespaceId
        )
    } catch (e: SyncException) {
        Log.e("Sync", "Error", e)
    }
}
```

### Exception Handling

The library throws `SyncException` with various subtypes:

```kotlin
try {
    run(...)
} catch (e: SyncException.Unauthorized) {
    // Handle authentication error
    Log.e("Sync", "Unauthorized: ${e.message}")
} catch (e: SyncException.IoException) {
    // Handle I/O error
    Log.e("Sync", "I/O error: ${e.message}")
} catch (e: SyncException) {
    // Handle other errors
    Log.e("Sync", "Sync error: ${e.message}")
}
```

### Best Practices (Android)

1. **Database Location**: Use `context.getDatabasePath()` to get proper database location
2. **Storage Directory**: Use `context.getExternalFilesDir()` for synced files
3. **Background Sync**: Use WorkManager for periodic background sync operations
4. **Coroutines**: Always run sync operations on `Dispatchers.IO` to avoid blocking main thread
5. **Lifecycle**: Cancel sync context in `onDestroy()` or when Activity/Fragment is destroyed
6. **Permissions**: Request `WRITE_EXTERNAL_STORAGE` permission if targeting Android < 10
7. **Read-Only Mode**: Set `downloadOnly = true` if you want to prevent local changes from syncing

Building bindings
=================

### Prepare

Install `rustup` https://www.rust-lang.org/tools/install.

Then add iOS targets.

    rustup target add aarch64-apple-ios
    rustup target add x86_64-apple-ios
    rustup target add aarch64-apple-ios-sim

Install iOS SDK https://developer.apple.com/xcode/resources/.

### Build XCFramework (Recommended)

The easiest way to build the Swift bindings is to use the provided build script:

    cd client
    ./scripts/build_swift_framework.sh cooklang-sync-client cooklang_sync_client CooklangSyncClientFFI

This will:
- Build the Rust library for iOS device and simulator targets
- Generate Swift bindings using UniFFI
- Create a universal XCFramework at `../swift/CooklangSyncClientFFI.xcframework`
- Set a valid CFBundleIdentifier (converts underscores to hyphens: `org.cooklang.cooklang-sync-client`)
- Create a zip archive of the XCFramework
- Calculate the SHA-256 checksum
- Automatically update `Package.swift` with the current version (from `Cargo.toml`) and checksum

**Note**: Apple's CFBundleIdentifier must only contain alphanumeric characters, hyphens, and periods.
The build script automatically converts underscores in the library name to hyphens to comply with this requirement.

After building, the script will display the version, checksum, and instructions for publishing a release.

### Using the Local XCFramework

To use the locally built XCFramework with the Swift Package instead of the remote release version, set the `USE_LOCAL_XCFRAMEWORK` environment variable:

```bash
export USE_LOCAL_XCFRAMEWORK=1
```

Then in your Xcode project that depends on this package, clean and rebuild. The Package.swift will automatically use the local XCFramework at `swift/CooklangSyncClientFFI.xcframework` instead of downloading from GitHub releases.

### Manual Build (Advanced)

Build library:

    cargo build --lib --target=x86_64-apple-ios --release
    cargo build --lib --target=aarch64-apple-ios --release

Build foreign language bindings (this will output Swift code into `./out` dir):

    cargo run --features="uniffi/cli"  \
      --bin uniffi-bindgen generate \
      --config uniffi.toml \
      --library ../target/x86_64-apple-ios/release/libcooklang_sync_client.a \
      --language swift \
      --out-dir out

See example of a Xcode project [here](https://github.com/cooklang/cooklang-ios/blob/main/Cooklang.xcodeproj).

Combine into universal library:

    mkdir -p ../target/universal/release
    mkdir -p ../target/universal_macos/release
    lipo -create -output ../target/universal/release/libcooklang_sync_client.a \
      ../target/x86_64-apple-ios/release/libcooklang_sync_client.a \
      ../target/aarch64-apple-ios/release/libcooklang_sync_client.a

    lipo -create -output ../target/universal_macos/release/libcooklang_sync_client.a \
      ../target/x86_64-apple-darwin/release/libcooklang_sync_client.a \
      ../target/aarch64-apple-darwin/release/libcooklang_sync_client.a


    xcodebuild -create-xcframework \
       -library ../target/aarch64-apple-ios/release/libcooklang_sync_client.a \
       -library ../target/x86_64-apple-ios/release/libcooklang_sync_client.a \
       -output CooklangSyncClientFFI.xcframework


       cp ../target/universal/release/libcooklang_sync_client.a ../swift/CooklangSyncClientFFI.xcframework/ios-arm64/CooklangSyncClientFFI.framework/CooklangSyncClientFFI

       cp ../target/universal/release/libcooklang_sync_client.a ../swift/CooklangSyncClientFFI.xcframework/ios-arm64_x86_64-simulator/CooklangSyncClientFFI.framework/CooklangSyncClient

TODO
====
- bundling of uploads/downloads
- read-only
- namespaces
- proper error handling
- report error on unexpeted cache behaviour
- don't need to throw unknown error in each non-200 response
- remove clone
- limit max file
- configuration struct
- pull changes first or reindex locally first? research possible conflict scenarios


- extract to core shared datasctuctures
- garbage collection on DB
- test test test
- metrics for monitoring (cache saturation, miss)
- protect from ddos https://github.com/rousan/multer-rs/blob/master/examples/prevent_dos_attack.rs
- auto-update client

open sourcing
=============
- how to keep it available for opensource (one user?)
- add documentation
- draw data-flow
