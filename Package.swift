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
        url: "https://github.com/cooklang/cooklang-sync/releases/download/v0.4.11/CooklangSyncFFI.xcframework.zip",
        checksum: "01bf34c8ac188ea2d0a43fa9d0aa3fe728cef3ae403079b40a9b8b5b61523576"))
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
