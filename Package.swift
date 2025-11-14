// swift-tools-version: 5.7
import PackageDescription
import class Foundation.ProcessInfo

let useLocalXCFramework = ProcessInfo.processInfo.environment["USE_LOCAL_XCFRAMEWORK"] != nil

var targets: [Target] = [
    .target(
        name: "CooklangSyncClient",
        dependencies: [.target(name: useLocalXCFramework ? "CooklangSyncClientFFI_local" : "CooklangSyncClientFFI")],
        path: "swift/Sources/CooklangSyncClient"),
]

if useLocalXCFramework {
    targets.append(.binaryTarget(
        name: "CooklangSyncClientFFI_local",
        path: "swift/CooklangSyncClientFFI.xcframework"))
} else {
    targets.append(.binaryTarget(
        name: "CooklangSyncClientFFI",
        url: "https://github.com/cooklang/cooklang-sync/releases/download/client-v0.3.0/CooklangSyncClientFFI.xcframework.zip",
        checksum: "b883f94e1cca165d93674ac089bf04e323b1ffb502fe6ef730ffbf590f00e5ce"))
}

let package = Package(
    name: "cooklang-sync-client",
    platforms: [
        .iOS(.v16),
    ],
    products: [
        .library(
            name: "CooklangSyncClient",
            targets: ["CooklangSyncClient"]),
    ],
    dependencies: [
    ],
    targets: targets
)
