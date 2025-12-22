#!/bin/bash
# Verify iOS XCFramework build

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
XCFRAMEWORK="$ROOT_DIR/swift/CooklangSyncFFI.xcframework"
SWIFT_SOURCES="$ROOT_DIR/swift/Sources/CooklangSync"

echo "=== iOS Build Verification ==="
echo ""

# Check XCFramework exists
echo "1. Checking XCFramework exists..."
if [ -d "$XCFRAMEWORK" ]; then
    echo "   ✓ XCFramework found at: $XCFRAMEWORK"
else
    echo "   ✗ XCFramework not found!"
    exit 1
fi

# Check architectures
echo ""
echo "2. Checking architectures..."
for arch_dir in "$XCFRAMEWORK"/*/; do
    arch_name=$(basename "$arch_dir")
    if [ "$arch_name" = "Info.plist" ]; then continue; fi

    binary="$arch_dir/CooklangSyncFFI.framework/CooklangSyncFFI"
    if [ -f "$binary" ]; then
        archs=$(lipo -info "$binary" 2>/dev/null | awk -F: '{print $NF}')
        size=$(ls -lh "$binary" | awk '{print $5}')
        echo "   ✓ $arch_name: $size ($archs)"
    fi
done

# Check Swift bindings
echo ""
echo "3. Checking Swift bindings..."
if [ -f "$SWIFT_SOURCES/CooklangSyncClient.swift" ]; then
    echo "   ✓ CooklangSyncClient.swift found"

    # Count public functions
    public_funcs=$(grep -c "^public func" "$SWIFT_SOURCES/CooklangSyncClient.swift" || echo "0")
    public_types=$(grep -c "^public \(class\|struct\|protocol\|enum\)" "$SWIFT_SOURCES/CooklangSyncClient.swift" || echo "0")
    echo "   ✓ Exported: $public_funcs functions, $public_types types"
else
    echo "   ✗ Swift bindings not found!"
    exit 1
fi

# Check headers
echo ""
echo "4. Checking headers..."
header="$XCFRAMEWORK/ios-arm64/CooklangSyncFFI.framework/Headers/CooklangSyncFFI.h"
if [ -f "$header" ]; then
    exports=$(grep -c "FFI_EXPORT" "$header" || echo "0")
    echo "   ✓ Header found with $exports FFI exports"
else
    echo "   ✗ Header not found!"
    exit 1
fi

# Check zip
echo ""
echo "5. Checking distribution package..."
zip_file="$ROOT_DIR/swift/CooklangSyncFFI.xcframework.zip"
if [ -f "$zip_file" ]; then
    size=$(ls -lh "$zip_file" | awk '{print $5}')
    echo "   ✓ Zip package: $size"
else
    echo "   ✗ Zip package not found!"
    exit 1
fi

# Test that the framework can be inspected by Xcode tools
echo ""
echo "6. Validating XCFramework structure..."
if xcodebuild -showSDKs &>/dev/null; then
    if plutil -lint "$XCFRAMEWORK/Info.plist" &>/dev/null; then
        echo "   ✓ Info.plist is valid"
    else
        echo "   ✗ Info.plist validation failed"
        exit 1
    fi
fi

echo ""
echo "=== All checks passed! ==="
echo ""
echo "Framework size summary:"
du -sh "$XCFRAMEWORK"
echo ""
echo "To use in your iOS project:"
echo "  1. Add CooklangSyncFFI.xcframework to your Xcode project"
echo "  2. Add CooklangSync Swift package or copy Sources/CooklangSync/*.swift"
echo "  3. Import CooklangSync in your Swift code"
