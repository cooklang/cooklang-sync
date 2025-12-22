#!/bin/bash
#
# Build Swift XCFramework from Rust library using UniFFI
#
# Usage: ./build_swift_framework.sh <package-name> <lib-name> <framework-name>
#
# Example: ./build_swift_framework.sh cooklang-sync-client cooklang_sync_client CooklangSyncFFI
#
# Note: The lib-name may contain underscores, but the CFBundleIdentifier will
# automatically convert them to hyphens to comply with Apple's naming requirements.

set -euo pipefail

PACKAGE=$1
LIB=$2
FRAMEWORK=$3
if [ -z "$PACKAGE" ]; then
  echo "$0: missing package name" >&2
  exit 1
fi
if [ -z "$LIB" ]; then
  echo "$0: missing library name" >&2
  exit 1
fi
if [ -z "$FRAMEWORK" ]; then
  echo "$0: missing framework name" >&2
  exit 1
fi

find_root() {
	RUST_ROOT=$(git rev-parse --show-toplevel)

	if [[ ! -d $RUST_ROOT ]]; then
		echo "$0: unable to find client directory" >&2
		exit 1
	fi
}

find_sim_triple() {
	case $(uname -m) in
	    x86_64)
	    	echo -n x86_64-apple-ios
		;;
	    aarch64 | arm64)
	    	echo -n aarch64-apple-ios-sim
		;;
	    *)
		echo "Unsupported architecture: $(uname -m)" >&2
		exit 1
		;;
	esac
}

build() {
	local target=$1

	# rustup only detects rust-toolchain.toml in the cwd
	(
		cd "$RUST_ROOT"
		rustup target add $target
        echo "Building $PACKAGE for $target"
		cargo build \
			--lib \
			--package=$PACKAGE \
			--release \
			--target=$target \
			--locked
	)
}

lib() {
	local framework_root=$1
	shift
	local targets=$*

	mkdir -p $framework_root
    echo "Creating universal library for $targets"
	lipo \
		-create $(printf "$BUILD_TARGET/%s/release/lib$LIB.a\n" $targets) \
		-output $framework_root/$FRAMEWORK
}

bindgen() {
	local target=$1
	local swift_root=$2

	(
		cd "$RUST_ROOT"
        echo "Generating bindings for $target"
		cargo run \
			--features="uniffi/cli" \
			--bin uniffi-bindgen \
			generate \
			--config uniffi.toml \
			--library $BUILD_TARGET/$target/release/lib$LIB.dylib \
			--language swift \
			--out-dir $swift_root
	)
}

header() {
	local framework_headers=$1/Headers
	mkdir -p $framework_headers

	local swift_root=$2
	cp $swift_root/$FRAMEWORK.h $framework_headers
}

modulemap() {
	local framework_modules=$1/Modules
	mkdir -p $framework_modules

	local swift_root=$2
	sed \
		-e "s/^module/framework module/" \
		$swift_root/$FRAMEWORK.modulemap > $framework_modules/module.modulemap
}

infoplist() {
  local framework_root=$1
  local plist="$framework_root/Info.plist"

  echo "Creating Info.plist for $plist"

  # Replace underscores with hyphens for valid bundle identifier
  local bundle_id=$(echo "$LIB" | tr '_' '-')

  if [ ! -f "$plist" ]; then
    /usr/libexec/PlistBuddy -c "Add :CFBundleDevelopmentRegion string en" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string $FRAMEWORK" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string org.cooklang.$bundle_id" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundleInfoDictionaryVersion string 6.0" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundlePackageType string FMWK" "$plist"
    # The following values are required. Without them, the App Store will return an "Asset validation failed" error.
    /usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string 1.0" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundleVersion string 1" "$plist"
    /usr/libexec/PlistBuddy -c "Add :MinimumOSVersion string 16.0" "$plist"
  fi
}

framework() {
	local framework_root=$RUST_BUILD_DIRECTORY/swift/.build/$1/$FRAMEWORK.framework
	shift
	local targets=$*

	for target in $targets; do
		build $target
	done

    echo "Creating universal framework for $framework_root"

	lib $framework_root $targets
	local swift_root=$RUST_BUILD_DIRECTORY/swift/Sources/CooklangSync
	bindgen $target $swift_root
	header $framework_root $swift_root
	modulemap $framework_root $swift_root
	infoplist $framework_root
}

xcframework() {
	local output=$RUST_BUILD_DIRECTORY/swift/$FRAMEWORK.xcframework

	[ -d "$output" ] && rm -rf "$output"
	xcodebuild \
		-create-xcframework \
		$(printf -- "-framework $RUST_BUILD_DIRECTORY/swift/.build/%s/$FRAMEWORK.framework\n" $*) \
		-output $output
}

clean() {
    rm -rf $RUST_BUILD_DIRECTORY/swift/Sources/**/*.h
    rm -rf $RUST_BUILD_DIRECTORY/swift/Sources/**/*.modulemap
    rm -rf $RUST_BUILD_DIRECTORY/swift/.build/
}

get_version() {
    # Extract version from Cargo.toml
    grep -m1 '^version = ' "$RUST_ROOT/client/Cargo.toml" | sed 's/version = "\(.*\)"/\1/'
}

zip_xcframework() {
    local xcframework_path=$RUST_BUILD_DIRECTORY/swift/$FRAMEWORK.xcframework
    local zip_path=$RUST_BUILD_DIRECTORY/swift/$FRAMEWORK.xcframework.zip

    echo "Creating zip archive..."
    [ -f "$zip_path" ] && rm "$zip_path"

    (cd "$RUST_BUILD_DIRECTORY/swift" && zip -r "$FRAMEWORK.xcframework.zip" "$FRAMEWORK.xcframework")

    echo "Zip created at: $zip_path"
}

calculate_checksum() {
    local zip_path=$RUST_BUILD_DIRECTORY/swift/$FRAMEWORK.xcframework.zip

    echo "Calculating checksum..." >&2
    shasum -a 256 "$zip_path" | awk '{print $1}'
}

update_package_swift() {
    local version=$1
    local checksum=$2
    local package_swift=$RUST_ROOT/Package.swift

    echo "Updating Package.swift with version $version and checksum $checksum"

    # Create a temporary file
    local temp_file=$(mktemp)

    # Update the URL and checksum in Package.swift
    # Note: macOS sed doesn't support {n} repetition, so we use a longer pattern
    sed -E \
        -e "s|url: \"https://github.com/cooklang/cooklang-sync/releases/download/v[0-9]+\.[0-9]+\.[0-9]+/CooklangSyncClientFFI\.xcframework\.zip\"|url: \"https://github.com/cooklang/cooklang-sync/releases/download/v${version}/CooklangSyncClientFFI.xcframework.zip\"|" \
        -e "s|checksum: \"[a-f0-9][a-f0-9]*\"|checksum: \"${checksum}\"|" \
        "$package_swift" > "$temp_file"

    # Replace the original file
    mv "$temp_file" "$package_swift"

    echo "Package.swift updated successfully"
}

find_root
RUST_BUILD_DIRECTORY=$RUST_ROOT
BUILD_TARGET=$RUST_BUILD_DIRECTORY/target

clean
framework ios aarch64-apple-ios
framework ios-sim aarch64-apple-ios-sim
xcframework ios ios-sim

# Zip the XCFramework and update Package.swift
VERSION=$(get_version)
zip_xcframework
CHECKSUM=$(calculate_checksum)
update_package_swift "$VERSION" "$CHECKSUM"

echo ""
echo "=========================================="
echo "Build completed successfully!"
echo "=========================================="
echo "Version: $VERSION"
echo "Checksum: $CHECKSUM"
echo "XCFramework: $RUST_BUILD_DIRECTORY/swift/$FRAMEWORK.xcframework"
echo "Zip: $RUST_BUILD_DIRECTORY/swift/$FRAMEWORK.xcframework.zip"
echo ""
echo "Package.swift has been updated with the new version and checksum."
echo "To publish, create a GitHub release with tag 'v$VERSION' and upload:"
echo "  $RUST_BUILD_DIRECTORY/swift/$FRAMEWORK.xcframework.zip"
echo "=========================================="

clean
