// Kanonak coordinate parsing — `publisher/package[@version]/name` (issue #17).
// Two operations, two strictness levels, both pinned by
// `vectors/coordinate-vectors.json`:
//
// - `parseCoordinate` (and the `versionlessKey` / `localName` conveniences) is
//   STRICT: it throws on anything that is not a well-formed coordinate rather
//   than returning a plausible-looking tail.
// - `lenientVersionlessKey` is the total, non-throwing structural variant that
//   carrier routing uses: it reduces a datatype URI to its versionless key
//   WITHOUT validating the version content, or returns nil when the input does
//   not have the three-segment shape. It exists so `carrierOf` stays total and
//   byte-identical under `canonicalFormVersion` "1" — an unparseable datatype
//   URI routes to no carrier (raw token preserved), never to a guess and never
//   to a crash.
//
// No ordering API on purpose: runtime consumers compare coordinates for
// equality only. Version ranges, compatibility, and resolution live in the
// SDK, not here. `#` is reserved for embedded resources and rejected by the
// strict parser.

import Foundation

public struct CoordinateVersion: Equatable {
    public let major: Int
    public let minor: Int
    public let patch: Int
    public init(major: Int, minor: Int, patch: Int) {
        self.major = major
        self.minor = minor
        self.patch = patch
    }
}

public struct Coordinate: Equatable {
    public let publisher: String
    public let `package`: String
    public let name: String
    /// nil for the versionless form (`publisher/package/name`).
    public let version: CoordinateVersion?
    public init(publisher: String, package: String, name: String, version: CoordinateVersion? = nil) {
        self.publisher = publisher
        self.package = package
        self.name = name
        self.version = version
    }
}

/// Exactly `0` or a digit run with no leading zero — one version part.
private func isVersionPart(_ s: Substring) -> Bool {
    guard !s.isEmpty, s.allSatisfy({ ("0"..."9").contains($0) }) else { return false }
    return s.count == 1 || s.first != "0"
}

private func invalid(_ uri: String, _ reason: String) -> CanonicalError {
    CanonicalError(
        "parseCoordinate: '\(uri)' is not a valid Kanonak coordinate (\(reason)); "
        + "expected publisher/package[@major.minor.patch]/name")
}

/// Parse a coordinate `publisher/package[@version]/name` into its parts.
/// Throws on malformed input — never returns a plausible-looking tail.
public func parseCoordinate(_ uri: String) throws -> Coordinate {
    if uri.isEmpty { throw invalid(uri, "empty string") }
    if uri.unicodeScalars.contains(where: { CharacterSet.whitespacesAndNewlines.contains($0) }) {
        throw invalid(uri, "whitespace is not allowed")
    }
    if uri.contains("#") {
        throw invalid(uri, "'#' is reserved for embedded resources")
    }

    let segments = uri.split(separator: "/", omittingEmptySubsequences: false)
    guard segments.count == 3 else {
        throw invalid(uri, "expected exactly 3 '/'-separated segments, got \(segments.count)")
    }
    let publisher = String(segments[0])
    let middle = String(segments[1])
    let name = String(segments[2])
    if publisher.isEmpty || middle.isEmpty || name.isEmpty {
        throw invalid(uri, "empty segment")
    }
    if publisher.contains("@") || name.contains("@") {
        throw invalid(uri, "'@' is only valid after the package name")
    }

    guard let at = middle.firstIndex(of: "@") else {
        return Coordinate(publisher: publisher, package: middle, name: name)
    }

    let pkg = String(middle[..<at])
    let versionPart = middle[middle.index(after: at)...]
    if pkg.isEmpty { throw invalid(uri, "empty package name before @") }
    if versionPart.contains("@") { throw invalid(uri, "more than one '@'") }

    let parts = versionPart.split(separator: ".", omittingEmptySubsequences: false)
    guard parts.count == 3, parts.allSatisfy(isVersionPart) else {
        throw invalid(
            uri, "version '\(versionPart)' is not exactly major.minor.patch (digits, no leading zeros)")
    }

    return Coordinate(
        publisher: publisher, package: pkg, name: name,
        version: CoordinateVersion(
            major: Int(parts[0])!, minor: Int(parts[1])!, patch: Int(parts[2])!))
}

/// The versionless canonical key `publisher/package/name` of a coordinate.
/// Strict: throws on malformed input.
public func versionlessKey(_ uri: String) throws -> String {
    let c = try parseCoordinate(uri)
    return "\(c.publisher)/\(c.package)/\(c.name)"
}

/// The local name of a coordinate. Strict: throws on malformed input.
public func localName(_ uri: String) throws -> String {
    try parseCoordinate(uri).name
}

/// Total structural reduction to the versionless key, for carrier routing:
/// three non-empty `/`-segments required, everything from the first `@` in the
/// middle segment discarded WITHOUT validating it (the part before the `@`
/// must be non-empty). Returns nil for anything else — never throws.
public func lenientVersionlessKey(_ uri: String) -> String? {
    let segments = uri.split(separator: "/", omittingEmptySubsequences: false)
    guard segments.count == 3 else { return nil }
    let publisher = segments[0], middle = segments[1], name = segments[2]
    if publisher.isEmpty || middle.isEmpty || name.isEmpty { return nil }
    let pkg = middle[..<(middle.firstIndex(of: "@") ?? middle.endIndex)]
    if pkg.isEmpty { return nil }
    return "\(publisher)/\(pkg)/\(name)"
}
