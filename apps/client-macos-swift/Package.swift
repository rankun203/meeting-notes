// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "GdayMeetings",
    platforms: [.macOS("14.2")],
    products: [.executable(name: "GdayMeetings", targets: ["GdayMeetings"])],
    targets: [
        .executableTarget(name: "GdayMeetings"),
        .testTarget(name: "GdayMeetingsTests", dependencies: ["GdayMeetings"])
    ]
)
