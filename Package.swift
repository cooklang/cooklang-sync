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
            url: "https://github.com/cooklang/cooklang-sync/releases/download/client-v0.2.7/CooklangSyncClientFFI.xcframework.zip",
            checksum: "b761b18119165f5efa041ce0459e6548db50ed11f07fa893715a73517059bf56"),
    ]
)

let cooklangSyncClientTarget = package.targets.first(where: { $0.name == "CooklangSyncClient" })

if ProcessInfo.processInfo.environment["USE_LOCAL_XCFRAMEWORK"] == nil {
    cooklangSyncClientTarget?.dependencies.append("CooklangSyncClientFFI")
} else {
    package.targets.append(.binaryTarget(
        name: "CooklangSyncClientFFI_local",
        path: "bindings/out/CooklangSyncClientFFI.xcframework"))

    cooklangSyncClientTarget?.dependencies.append("CooklangSyncClientFFI_local")
}
