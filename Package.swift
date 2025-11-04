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
        url: "https://github.com/cooklang/cooklang-sync/releases/download/client-v0.2.10/CooklangSyncClientFFI.xcframework.zip",
        checksum: "6af279cb1417dbba1044228e016b658fcf3952abc058496319f592def831362e"))
}

let package = Package(
    name: "cooklang-sync-client",
    platforms: [
        .iOS(.v15),
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
