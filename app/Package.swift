// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "PortsBar",
    platforms: [
        .macOS(.v14)
    ],
    targets: [
        // All app logic + the SwiftUI App scene live here so they are
        // unit-testable. swift-testing cannot be hosted by a test target that
        // links an executable's `@main`, hence the library/executable split.
        // The library uses its DEFAULT path (Sources/PortsBarCore); giving it an
        // explicit path while the executable also has a custom path made SwiftPM
        // map both targets under Sources/PortsBarMain ("no such module").
        .target(
            name: "PortsBarCore",
            swiftSettings: [
                .swiftLanguageMode(.v6)
            ]
        ),
        // Thin executable shim: just calls PortsBarApp.main().
        .executableTarget(
            name: "PortsBar",
            dependencies: ["PortsBarCore"],
            path: "Sources/PortsBarMain",
            swiftSettings: [
                .swiftLanguageMode(.v6)
            ]
        ),
        .testTarget(
            name: "PortsBarTests",
            dependencies: ["PortsBarCore"],
            swiftSettings: [
                .swiftLanguageMode(.v6)
            ]
        )
    ]
)
