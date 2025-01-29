#!/bin/bash

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

  if [ ! -f "$plist" ]; then
    /usr/libexec/PlistBuddy -c "Add :CFBundleDevelopmentRegion string en" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string $FRAMEWORK" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string build.wallet.rust.$LIB" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundleInfoDictionaryVersion string 6.0" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundlePackageType string FMWK" "$plist"
    # The following values are required. Without them, the App Store will return an "Asset validation failed" error.
    /usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string 1.0" "$plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundleVersion string 1" "$plist"
    /usr/libexec/PlistBuddy -c "Add :MinimumOSVersion string 15.2" "$plist"
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
	local swift_root=$RUST_BUILD_DIRECTORY/swift/Sources/CooklangSyncClient
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

find_root
RUST_BUILD_DIRECTORY=$RUST_ROOT
BUILD_TARGET=$RUST_BUILD_DIRECTORY/target

clean
framework ios aarch64-apple-ios
framework ios-sim $(find_sim_triple)
xcframework ios ios-sim
clean
