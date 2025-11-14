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
        checksum: "da07f5db9740969092f534b9c3e9e465700af13498a5a4c49c1ae5dfce2c7817"))
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
