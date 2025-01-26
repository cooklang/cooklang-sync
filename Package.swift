// swift-tools-version: 5.7
import PackageDescription
import class Foundation.ProcessInfo

var package = Package(
    name: "cooklang-sync-client",
    platforms: [
        .macOS(.v10_15),
        .iOS(.v15),
    ],
    products: [
        .library(
            name: "CooklangSyncClient",
            targets: ["CooklangSyncClient"]),
    ],
    dependencies: [
    ],
    targets: [
        .target(
            name: "CooklangSyncClient",
            path: "swift/Sources/CooklangSyncClient"),
        .testTarget(
            name: "CooklangSyncClientTests",
            dependencies: ["CooklangSyncClient"],
            path: "swift/Tests/CooklangSyncClientTests"),
        .binaryTarget(
            name: "CooklangSyncClientFFI",
            url: "https://github.com/cooklang/cooklang-sync/releases/download/client-v0.2.5/CooklangSyncClientFFI.xcframework.zip",
            checksum: "1f52679045e61166d3084d18366f2a8928adee3fefa442c1bebe5f20564861f3"),
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
