#!/bin/bash
#
# Build Android library from Rust library using UniFFI
#
# Usage: ./build-android.sh
#
# Prerequisites:
#   - Android NDK installed (set ANDROID_NDK_HOME or use default path)
#   - cargo-ndk installed: cargo install cargo-ndk
#   - Rust Android targets: rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
#

set -euo pipefail

PACKAGE="cooklang-sync-client"
LIB="cooklang_sync_client"

find_root() {
    RUST_ROOT=$(git rev-parse --show-toplevel)

    if [[ ! -d $RUST_ROOT ]]; then
        echo "$0: unable to find repository root" >&2
        exit 1
    fi
}

check_prerequisites() {
    if ! command -v cargo-ndk &> /dev/null; then
        echo "cargo-ndk is not installed. Install it with: cargo install cargo-ndk"
        exit 1
    fi

    if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
        # Try to find NDK in common locations
        if [[ -d "$HOME/Library/Android/sdk/ndk" ]]; then
            # Find latest NDK version
            ANDROID_NDK_HOME=$(ls -d "$HOME/Library/Android/sdk/ndk"/*/ 2>/dev/null | sort -V | tail -1 | sed 's:/$::')
        elif [[ -d "$HOME/Android/Sdk/ndk" ]]; then
            ANDROID_NDK_HOME=$(ls -d "$HOME/Android/Sdk/ndk"/*/ 2>/dev/null | sort -V | tail -1 | sed 's:/$::')
        fi

        if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
            echo "ANDROID_NDK_HOME is not set and could not be auto-detected"
            echo "Please set ANDROID_NDK_HOME to your Android NDK installation path"
            exit 1
        fi
        echo "Auto-detected ANDROID_NDK_HOME: $ANDROID_NDK_HOME"
    fi

    export ANDROID_NDK_HOME
}

install_targets() {
    echo "Installing Android targets..."
    rustup target add aarch64-linux-android
    rustup target add armv7-linux-androideabi
    rustup target add x86_64-linux-android
}

build() {
    local target=$1
    local platform=${2:-21}

    echo "Building $PACKAGE for $target (API level $platform)..."
    (
        cd "$RUST_ROOT"
        cargo ndk --target "$target" --platform "$platform" build --release --package "$PACKAGE"
    )
}

generate_bindings() {
    local output_dir=$1

    echo "Generating Kotlin bindings..."
    mkdir -p "$output_dir"

    (
        cd "$RUST_ROOT"
        cargo build --package "$PACKAGE" --features uniffi/cli --bin uniffi-bindgen --release
        ./target/release/uniffi-bindgen generate \
            --library "target/aarch64-linux-android/release/lib${LIB}.so" \
            --language kotlin \
            --config client/uniffi.toml \
            --out-dir "$output_dir"
    )

    echo "Generated Kotlin files:"
    find "$output_dir" -name "*.kt" -type f
}

organize_jni_libs() {
    local output_dir=$1

    echo "Organizing JNI libraries..."
    mkdir -p "$output_dir"/{arm64-v8a,armeabi-v7a,x86_64}

    cp "$BUILD_TARGET/aarch64-linux-android/release/lib${LIB}.so" "$output_dir/arm64-v8a/"
    cp "$BUILD_TARGET/armv7-linux-androideabi/release/lib${LIB}.so" "$output_dir/armeabi-v7a/"
    cp "$BUILD_TARGET/x86_64-linux-android/release/lib${LIB}.so" "$output_dir/x86_64/"
}

create_android_module() {
    local output_dir=$1
    local jni_libs_dir=$2
    local kotlin_dir=$3

    echo "Creating Android library module..."
    mkdir -p "$output_dir/src/main/kotlin"
    mkdir -p "$output_dir/src/main/jniLibs"

    # Copy JNI libs
    cp -R "$jni_libs_dir"/* "$output_dir/src/main/jniLibs/"

    # Copy Kotlin bindings
    find "$kotlin_dir" -name "*.kt" -exec cp {} "$output_dir/src/main/kotlin/" \;

    # Create build.gradle.kts
    cat > "$output_dir/build.gradle.kts" << 'EOF'
plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "org.cooklang.sync"
    compileSdk = 34

    defaultConfig {
        minSdk = 21
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }
}

dependencies {
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.7.3")
}
EOF

    # Create AndroidManifest.xml
    mkdir -p "$output_dir/src/main"
    cat > "$output_dir/src/main/AndroidManifest.xml" << 'EOF'
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-permission android:name="android.permission.INTERNET" />
</manifest>
EOF

    # Create proguard rules
    cat > "$output_dir/proguard-rules.pro" << 'EOF'
-keep class uniffi.** { *; }
-keep class org.cooklang.** { *; }
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
EOF

    cat > "$output_dir/consumer-rules.pro" << 'EOF'
-keep class uniffi.** { *; }
-keep class org.cooklang.** { *; }
EOF
}

get_version() {
    grep -m1 '^version = ' "$RUST_ROOT/client/Cargo.toml" | sed 's/version = "\(.*\)"/\1/'
}

clean() {
    echo "Cleaning up..."
    rm -rf "$RUST_BUILD_DIRECTORY/android"
}

# Main execution
find_root
check_prerequisites

RUST_BUILD_DIRECTORY=$RUST_ROOT
BUILD_TARGET=$RUST_BUILD_DIRECTORY/target
ANDROID_OUTPUT=$RUST_BUILD_DIRECTORY/android

# Clean previous build
clean

# Install targets if needed
install_targets

# Build for all architectures
build aarch64-linux-android
build armv7-linux-androideabi
build x86_64-linux-android

# Generate bindings and organize output
generate_bindings "$ANDROID_OUTPUT/kotlin"
organize_jni_libs "$ANDROID_OUTPUT/jniLibs"
create_android_module "$ANDROID_OUTPUT/cooklang-sync-android" "$ANDROID_OUTPUT/jniLibs" "$ANDROID_OUTPUT/kotlin"

VERSION=$(get_version)

echo ""
echo "=========================================="
echo "Android build completed successfully!"
echo "=========================================="
echo "Version: $VERSION"
echo "Output directory: $ANDROID_OUTPUT"
echo ""
echo "Contents:"
echo "  - cooklang-sync-android/ - Android library module"
echo "  - jniLibs/               - Native libraries"
echo "  - kotlin/                - Kotlin bindings"
echo ""
echo "To use in your Android project:"
echo "  1. Copy cooklang-sync-android/ to your project"
echo "  2. Add it as a module in settings.gradle.kts"
echo "  3. Add implementation project(':cooklang-sync-android') to your app's build.gradle.kts"
echo "=========================================="
