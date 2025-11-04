// swift-tools-version: 5.7
import PackageDescription
import class Foundation.ProcessInfo

var package = Package(
    name: "cooklang-sync-client",
    platforms: [
        .iOS(.v15),
    ],
    products: [
        .library(
            name: "CooklangSyncClient",
            targets: ["CooklangSyncClient", "CooklangSyncClientFFI"]),
    ],
    dependencies: [
    ],
    targets: [
        .target(
            name: "CooklangSyncClient",
            path: "swift/Sources/CooklangSyncClient"),
        .binaryTarget(
            name: "CooklangSyncClientFFI",
            url: "https://github.com/cooklang/cooklang-sync/releases/download/client-v0.2.10/CooklangSyncClientFFI.xcframework.zip",
            checksum: "6af279cb1417dbba1044228e016b658fcf3952abc058496319f592def831362e"),
    ]
)

let cooklangSyncClientTarget = package.targets.first(where: { $0.name == "CooklangSyncClient" })

if ProcessInfo.processInfo.environment["USE_LOCAL_XCFRAMEWORK"] == nil {
    cooklangSyncClientTarget?.dependencies.append("CooklangSyncClientFFI")
} else {
    package.targets.append(.binaryTarget(
        name: "CooklangSyncClientFFI_local",
        path: "swift/CooklangSyncClientFFI.xcframework"))

    cooklangSyncClientTarget?.dependencies.append("CooklangSyncClientFFI_local")
}
