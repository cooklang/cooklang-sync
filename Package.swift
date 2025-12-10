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
        url: "https://github.com/cooklang/cooklang-sync/releases/download/v0.4.0/CooklangSyncClientFFI.xcframework.zip",
        checksum: "9f50dd23784af802bd5c6c74504a3c2bcf25d33e6dfb383c31e88fa4fd350b0f"))
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
