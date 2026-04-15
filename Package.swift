// swift-tools-version: 5.7
import PackageDescription
import class Foundation.ProcessInfo

let useLocalXCFramework = ProcessInfo.processInfo.environment["USE_LOCAL_XCFRAMEWORK"] != nil

var targets: [Target] = [
    .target(
        name: "CooklangSync",
        dependencies: [.target(name: useLocalXCFramework ? "CooklangSyncFFI_local" : "CooklangSyncFFI")],
        path: "swift/Sources/CooklangSync"),
]

if useLocalXCFramework {
    targets.append(.binaryTarget(
        name: "CooklangSyncFFI_local",
        path: "swift/CooklangSyncFFI.xcframework"))
} else {
    targets.append(.binaryTarget(
        name: "CooklangSyncFFI",
        url: "https://github.com/cooklang/cooklang-sync/releases/download/v0.4.12/CooklangSyncFFI.xcframework.zip",
        checksum: "111cc889b9a3e99597d6d795e20e50272f54fdd7b5b23eb984c5f9203939fe35"))
}

let package = Package(
    name: "cooklang-sync",
    platforms: [
        .iOS(.v16),
    ],
    products: [
        .library(
            name: "CooklangSyncClient",
            targets: ["CooklangSync"]),
    ],
    dependencies: [
    ],
    targets: targets
)
