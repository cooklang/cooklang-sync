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
            checksum: "330dee190d62d1784fd458653e6d94191773f6747a591358b29373f50b666805"),
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
