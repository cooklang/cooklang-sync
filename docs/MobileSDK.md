# Mobile SDK Integration Guide

This guide covers how to integrate the Cooklang Sync client library into iOS and Android applications.

## iOS (Swift Package Manager)

### Installation

Add the package to your Xcode project or `Package.swift`:

**Using Xcode:**
1. File → Add Package Dependencies
2. Enter: `https://github.com/cooklang/cooklang-sync.git`
3. Select version rule (e.g., "Up to Next Major Version")

**Using Package.swift:**

```swift
dependencies: [
    .package(url: "https://github.com/cooklang/cooklang-sync.git", from: "0.4.0")
]
```

Then add `CooklangSync` to your target dependencies:

```swift
.target(
    name: "YourApp",
    dependencies: ["CooklangSync"]
)
```

### Usage

```swift
import CooklangSync

// Initialize the sync client
// (API details depend on the exposed UniFFI interface)
```

### Requirements

- iOS 16.0+
- Swift 5.7+

### Local Development

To use a locally built XCFramework instead of the release binary:

```bash
export USE_LOCAL_XCFRAMEWORK=1
```

Then build with Swift Package Manager as usual. This will use the XCFramework at `swift/CooklangSyncFFI.xcframework`.

## Android (Gradle)

### Installation via GitHub Packages

1. Add the GitHub Packages Maven repository to your `settings.gradle.kts`:

```kotlin
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
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

2. Add the dependency to your module's `build.gradle.kts`:

```kotlin
dependencies {
    implementation("org.cooklang:cooklang-sync:0.4.4")
}
```

3. Configure GitHub authentication:

**Option A: Environment variables**
```bash
export GITHUB_ACTOR=your-github-username
export GITHUB_TOKEN=your-personal-access-token
```

**Option B: gradle.properties** (in `~/.gradle/gradle.properties`)
```properties
gpr.user=your-github-username
gpr.key=your-personal-access-token
```

The token needs the `read:packages` scope.

### Manual Installation

Alternatively, download the Android artifacts from the [GitHub Releases](https://github.com/cooklang/cooklang-sync/releases) page:

1. Download `cooklang-sync-android.zip`
2. Extract and copy `cooklang-sync-android/` to your project
3. Add it as a module in `settings.gradle.kts`:

```kotlin
include(":cooklang-sync-android")
```

4. Add the dependency:

```kotlin
dependencies {
    implementation(project(":cooklang-sync-android"))
}
```

### Usage

```kotlin
import org.cooklang.sync.*

// Initialize the sync client
// (API details depend on the exposed UniFFI interface)
```

### Requirements

- Android SDK 21+ (Android 5.0 Lollipop)
- Kotlin 1.9+

### ProGuard / R8

The library includes consumer ProGuard rules automatically. If you need to customize, ensure these classes are kept:

```proguard
-keep class uniffi.** { *; }
-keep class org.cooklang.** { *; }
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
```

## Supported Architectures

### iOS
- `arm64` (devices)
- `arm64` (simulator, Apple Silicon)
- `x86_64` (simulator, Intel)

### Android
- `arm64-v8a` (most modern devices)
- `armeabi-v7a` (older 32-bit devices)
- `x86_64` (emulators, some Chromebooks)

## Building from Source

### Prerequisites

- Rust toolchain (latest stable)
- For iOS: Xcode with command-line tools
- For Android: Android NDK, `cargo-ndk`

### Build Commands

```bash
# Install dependencies
make install-deps

# Build iOS XCFramework
make ios

# Build Android library
make android

# Build both
make all

# Generate bindings only
make bindings-swift
make bindings-kotlin
```

### Android NDK Setup

Ensure `ANDROID_NDK_HOME` is set, or install the NDK via Android Studio:

```bash
# macOS (Android Studio default location)
export ANDROID_NDK_HOME="$HOME/Library/Android/sdk/ndk/<version>"

# Linux
export ANDROID_NDK_HOME="$HOME/Android/Sdk/ndk/<version>"
```

## Versioning

The library follows semantic versioning. iOS and Android packages are released together with the same version number.

## Troubleshooting

### iOS: "No such module 'CooklangSync'"

Ensure you've added the package dependency correctly and the minimum deployment target is iOS 16.0.

### Android: "Could not resolve org.cooklang:cooklang-sync"

1. Check that the GitHub Packages repository is configured
2. Verify your GitHub token has `read:packages` scope
3. Try `./gradlew --refresh-dependencies`

### Android: "UnsatisfiedLinkError"

The native library may not be included for your device's architecture. Check that your APK includes the required ABI in `jniLibs/`.
