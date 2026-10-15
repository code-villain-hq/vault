// swift-tools-version: 6.4
import PackageDescription

let package = Package(
    name: "VillainCaster",
    platforms: [.macOS(.v27)],
    targets: [
        .executableTarget(
            name: "VillainCaster",
            path: "Sources/VillainCaster",
            swiftSettings: [.swiftLanguageMode(.v5)]
        )
    ]
)
