#!/bin/bash

set -euo pipefail

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
"$SCRIPT_DIR"/build_swift_framework.sh cooklang-sync-client cooklang_sync_client CooklangSyncClientFFI


cd ../swift && zip -r CooklangSyncClientFFI.xcframework.zip CooklangSyncClientFFI.xcframework
shasum -a 256 ../swift/CooklangSyncClientFFI.xcframework.zip > ../swift/CooklangSyncClientFFI.xcframework.zip.sha256
