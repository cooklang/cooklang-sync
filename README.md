# Cooklang Sync

A file synchronization library for text files and small binary files (<10MB). Based on [Dropbox's streaming file synchronization design](https://dropbox.tech/infrastructure/streaming-file-synchronization).

## Installation

### iOS (Swift Package Manager)

Add to your `Package.swift`:

```swift
dependencies: [
    .package(url: "https://github.com/cooklang/cooklang-sync.git", from: "0.4.1")
]
```

Or in Xcode: File → Add Package Dependencies → enter `https://github.com/cooklang/cooklang-sync.git`

### Android (Gradle)

Add the GitHub Packages repository to `settings.gradle.kts`:

```kotlin
dependencyResolutionManagement {
    repositories {
        maven {
            url = uri("https://maven.pkg.github.com/cooklang/cooklang-sync")
            credentials {
                username = project.findProperty("gpr.user") as String? ?: System.getenv("GITHUB_ACTOR")
                password = project.findProperty("gpr.key") as String? ?: System.getenv("GITHUB_TOKEN")
            }
        }
    }
}
```

Add the dependency to `build.gradle.kts`:

```kotlin
dependencies {
    implementation("com.github.cooklang:sync:0.4.1")
}
```

See [Mobile SDK Integration Guide](docs/MobileSDK.md) for detailed setup instructions.

## Development

### Prerequisites

- Rust (latest stable)
- For iOS builds: Xcode with command-line tools
- For Android builds: Android NDK, cargo-ndk

### Building

```bash
# Install build dependencies
make install-deps

# Build iOS XCFramework
make ios

# Build Android library
make android

# Build both platforms
make all
```

### Running the Server

```bash
cargo run
```

### Running the Client

```bash
cargo run --bin client <sync-directory> <db-path> <server-url> <jwt-token>
```

Example:
```bash
cargo run --bin client ../tmp ./db/client.sqlite3 http://localhost:8000 eyXX.XXX.XXX
```

### JWT Token Generation

Test tokens can be generated at https://jwt.io with:
- Secret: `secret`
- Payload:
```json
{
  "uid": 100,
  "exp": 1720260223
}
```

## Architecture

The library consists of two main components:

- **Server** (`cooklang-sync-server`): Handles file synchronization requests
- **Client** (`cooklang-sync-client`): Provides sync capabilities with FFI bindings for iOS and Android

## Roadmap

- [ ] Garbage collection for client
- [ ] Comprehensive test suite
- [ ] Generalize for broader use cases
- [ ] Read-only file support
- [ ] Metrics export
- [ ] Security audit
- [ ] Symlink support
- [ ] Hidden file handling

## License

MPL-2.0
