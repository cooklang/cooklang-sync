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
        url: "https://github.com/cooklang/cooklang-sync/releases/download/v0.6.1/CooklangSyncFFI.xcframework.zip",
        checksum: "ba5ffab40fd9d60b57ff0c1a458444b7e84a6a2211c52597033fe13c40bc8e81"))
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
