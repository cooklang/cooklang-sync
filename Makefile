# Makefile for cooklang-sync mobile SDK builds
#
# Usage:
#   make ios        - Build iOS XCFramework
#   make android    - Build Android library
#   make all        - Build both iOS and Android
#   make clean      - Clean build artifacts
#   make install-deps - Install build dependencies

.PHONY: all ios android clean install-deps install-ios-deps install-android-deps bindings-swift bindings-kotlin help

# Package configuration
PACKAGE_NAME := cooklang-sync-client
LIB_NAME := cooklang_sync_client
FRAMEWORK_NAME := CooklangSyncFFI

# Paths
ROOT_DIR := $(shell pwd)
CLIENT_DIR := $(ROOT_DIR)/client
SCRIPTS_DIR := $(CLIENT_DIR)/scripts
TARGET_DIR := $(ROOT_DIR)/target

# iOS targets
IOS_TARGETS := aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# Android targets
ANDROID_TARGETS := aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

help:
	@echo "Cooklang Sync Mobile SDK Build System"
	@echo ""
	@echo "Usage:"
	@echo "  make ios           - Build iOS XCFramework"
	@echo "  make android       - Build Android library"
	@echo "  make all           - Build both iOS and Android"
	@echo "  make clean         - Clean build artifacts"
	@echo "  make install-deps  - Install all build dependencies"
	@echo ""
	@echo "Individual targets:"
	@echo "  make install-ios-deps     - Install iOS build dependencies"
	@echo "  make install-android-deps - Install Android build dependencies"
	@echo "  make bindings-swift       - Generate Swift bindings only"
	@echo "  make bindings-kotlin      - Generate Kotlin bindings only"

all: ios android

# iOS build
ios:
	@echo "Building iOS XCFramework..."
	cd $(SCRIPTS_DIR) && ./build_swift_framework.sh $(PACKAGE_NAME) $(LIB_NAME) $(FRAMEWORK_NAME)

# Android build
android:
	@echo "Building Android library..."
	cd $(SCRIPTS_DIR) && ./build-android.sh

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	rm -rf $(TARGET_DIR)/ios
	rm -rf $(TARGET_DIR)/android
	rm -rf $(ROOT_DIR)/android
	rm -rf $(ROOT_DIR)/swift/.build
	rm -rf $(ROOT_DIR)/swift/$(FRAMEWORK_NAME).xcframework
	rm -rf $(ROOT_DIR)/swift/$(FRAMEWORK_NAME).xcframework.zip
	cargo clean

# Install all dependencies
install-deps: install-ios-deps install-android-deps
	@echo "All dependencies installed."

# Install iOS dependencies
install-ios-deps:
	@echo "Installing iOS build dependencies..."
	rustup target add $(IOS_TARGETS)
	@echo "iOS dependencies installed."

# Install Android dependencies
install-android-deps:
	@echo "Installing Android build dependencies..."
	rustup target add $(ANDROID_TARGETS)
	@command -v cargo-ndk > /dev/null || cargo install cargo-ndk --locked
	@echo "Android dependencies installed."
	@echo ""
	@echo "Note: Make sure ANDROID_NDK_HOME is set or Android NDK is installed in a standard location."

# Generate Swift bindings only
bindings-swift:
	@echo "Generating Swift bindings..."
	@mkdir -p $(TARGET_DIR)/bindings/swift
	cargo build --package $(PACKAGE_NAME) --release --target aarch64-apple-ios
	cargo run --package $(PACKAGE_NAME) --features uniffi/cli --bin uniffi-bindgen -- generate \
		--library $(TARGET_DIR)/aarch64-apple-ios/release/lib$(LIB_NAME).a \
		--language swift \
		--config $(CLIENT_DIR)/uniffi.toml \
		--out-dir $(TARGET_DIR)/bindings/swift
	@echo "Swift bindings generated at: $(TARGET_DIR)/bindings/swift"

# Generate Kotlin bindings only
bindings-kotlin:
	@echo "Generating Kotlin bindings..."
	@mkdir -p $(TARGET_DIR)/bindings/kotlin
	@if [ -z "$$ANDROID_NDK_HOME" ]; then \
		echo "Warning: ANDROID_NDK_HOME not set, trying to auto-detect..."; \
	fi
	cargo ndk --target aarch64-linux-android --platform 21 build --release --package $(PACKAGE_NAME)
	cargo build --package $(PACKAGE_NAME) --features uniffi/cli --bin uniffi-bindgen --release
	$(TARGET_DIR)/release/uniffi-bindgen generate \
		--library $(TARGET_DIR)/aarch64-linux-android/release/lib$(LIB_NAME).so \
		--language kotlin \
		--config $(CLIENT_DIR)/uniffi.toml \
		--out-dir $(TARGET_DIR)/bindings/kotlin
	@echo "Kotlin bindings generated at: $(TARGET_DIR)/bindings/kotlin"

# Version info
version:
	@grep -m1 '^version = ' $(CLIENT_DIR)/Cargo.toml | sed 's/version = "\(.*\)"/\1/'
