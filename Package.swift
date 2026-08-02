// swift-tools-version: 5.9
// The Swift ports of the Kanonak runtime libraries, exposed as one SwiftPM
// package at the repo root (SwiftPM resolves git-URL dependencies only against
// a root manifest). Consumers depend on the repo URL and pick the product(s)
// they need; each product's sources live beside that member's other language
// ports (kanonak-canonical/swift, kanonak-codec/swift).
import PackageDescription

let package = Package(
    name: "kanonak-runtime",
    platforms: [.macOS(.v13), .iOS(.v16)],
    products: [
        .library(name: "KanonakCanonical", targets: ["KanonakCanonical"]),
        .library(name: "KanonakCodec", targets: ["KanonakCodec"]),
        .library(name: "KanonakExpression", targets: ["KanonakExpression"]),
        .library(name: "KanonakWire", targets: ["KanonakWire"]),
    ],
    dependencies: [
        // SHA-256 on Darwin and Linux through one import (`Crypto`).
        .package(url: "https://github.com/apple/swift-crypto.git", from: "3.0.0"),
    ],
    targets: [
        .target(
            name: "KanonakCanonical",
            dependencies: [.product(name: "Crypto", package: "swift-crypto")],
            path: "kanonak-canonical/swift/Sources/KanonakCanonical"
        ),
        .testTarget(
            name: "KanonakCanonicalTests",
            dependencies: ["KanonakCanonical"],
            path: "kanonak-canonical/swift/Tests/KanonakCanonicalTests"
        ),
        .target(
            name: "KanonakCodec",
            dependencies: ["KanonakCanonical"],
            path: "kanonak-codec/swift/Sources/KanonakCodec"
        ),
        .testTarget(
            name: "KanonakCodecTests",
            dependencies: ["KanonakCodec"],
            path: "kanonak-codec/swift/Tests/KanonakCodecTests"
        ),
        // Independent of canonical/codec — the expression kernel is a pure
        // operator engine over the dictionary node contract.
        .target(
            name: "KanonakExpression",
            path: "kanonak-expression/swift/Sources/KanonakExpression"
        ),
        .testTarget(
            name: "KanonakExpressionTests",
            dependencies: ["KanonakExpression"],
            path: "kanonak-expression/swift/Tests/KanonakExpressionTests"
        ),
        // Independent of every other member — the wire kernel is pure bytes.
        .target(
            name: "KanonakWire",
            path: "kanonak-wire/swift/Sources/KanonakWire"
        ),
        .testTarget(
            name: "KanonakWireTests",
            dependencies: ["KanonakWire"],
            path: "kanonak-wire/swift/Tests/KanonakWireTests"
        ),
    ]
)
