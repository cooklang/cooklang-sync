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
        url: "https://github.com/cooklang/cooklang-sync/releases/download/v0.4.7/CooklangSyncFFI.xcframework.zip",
        checksum: "271691a777f238088d82082d633fec3da8f25f6e5b3ac10028decfd16becfedb"))
}

let package = Package(
    name: "cooklang-sync",
    platforms: [
        .iOS(.v16),
    ],
    products: [
        .library(
            name: "CooklangSync",
            targets: ["CooklangSync"]),
    ],
    dependencies: [
    ],
    targets: targets
)
