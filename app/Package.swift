// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "PortsBar",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "PortsBar",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "PortsBarTests",
            dependencies: ["PortsBar"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
