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
        url: "https://github.com/cooklang/cooklang-sync/releases/download/v0.4.8/CooklangSyncFFI.xcframework.zip",
        checksum: "a6b6e47fb2a9ef60540bfd9d2f3b501754c56677657ca2a4aedd46e845c87dca"))
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
