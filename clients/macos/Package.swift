// swift-tools-version: 5.9
import PackageDescription
import Foundation

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
let nativeLibrary = ProcessInfo.processInfo.environment["SEATTRELLIS_NATIVE_LIB_DIR"]
    ?? root.appendingPathComponent("target/release").path

let package = Package(
    name: "SeatTrellisMac",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "SeatTrellisMac", targets: ["SeatTrellisMac"]),
        .library(name: "SeatTrellisKit", targets: ["SeatTrellisKit"]),
    ],
    targets: [
        .target(name: "CSeattrellis", publicHeadersPath: "include"),
        .target(name: "SeatTrellisKit", dependencies: ["CSeattrellis"], linkerSettings: [
            // Cargo also emits a dylib. An explicit archive prevents dyld
            // dependencies on the checkout from leaking into the app bundle.
            .unsafeFlags(["-Xlinker", nativeLibrary + "/libseattrellis_bridge.a"]),
            .linkedLibrary("c++"), .linkedLibrary("iconv"), .linkedLibrary("resolv"),
            .linkedFramework("Security"), .linkedFramework("CoreFoundation"),
            .linkedFramework("CoreGraphics"), .linkedFramework("CoreText"),
            .linkedFramework("AppKit"), .linkedFramework("SystemConfiguration"),
        ]),
        .executableTarget(name: "SeatTrellisMac", dependencies: ["SeatTrellisKit"]),
        .testTarget(name: "SeatTrellisKitTests", dependencies: ["SeatTrellisKit"]),
    ],
    swiftLanguageVersions: [.v5]
)
