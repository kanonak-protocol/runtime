// Kanonak coordinate parsing — `publisher/package[@version]/name` (issue #17),
// and a package's own coordinate, `publisher/package@version` (runtime#53).
// Two operations, two strictness levels, both pinned by
// `vectors/coordinate-vectors.json`:
//
// - `parseCoordinate` (and the `versionlessKey` / `localName` conveniences) is
//   STRICT: it throws on anything that is not a well-formed coordinate rather
//   than returning a plausible-looking tail.
// - `parsePackageCoordinate` is the same STRICT grammar for a package at a
//   version, as a lock names one: no `/name`, and the version required.
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

public struct PackageCoordinate: Equatable {
    public let publisher: String
    public let `package`: String
    public let version: CoordinateVersion
    public init(publisher: String, package: String, version: CoordinateVersion) {
        self.publisher = publisher
        self.package = package
        self.version = version
    }
}

/// Exactly `0` or a digit run with no leading zero — one version part.
private func isVersionPart(_ s: Substring) -> Bool {
    guard !s.isEmpty, s.allSatisfy({ ("0"..."9").contains($0) }) else { return false }
    return s.count == 1 || s.first != "0"
}

/// `major.minor.patch`, each part an `isVersionPart`, or nil.
private func parseVersion(_ text: Substring) -> CoordinateVersion? {
    let parts = text.split(separator: ".", omittingEmptySubsequences: false)
    guard parts.count == 3, parts.allSatisfy(isVersionPart) else { return nil }
    return CoordinateVersion(major: Int(parts[0])!, minor: Int(parts[1])!, patch: Int(parts[2])!)
}

private func versionReason(_ text: Substring) -> String {
    "version '\(text)' is not exactly major.minor.patch (digits, no leading zeros)"
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

    guard let version = parseVersion(versionPart) else {
        throw invalid(uri, versionReason(versionPart))
    }
    return Coordinate(publisher: publisher, package: pkg, name: name, version: version)
}

private func invalidPackage(_ coordinate: String, _ reason: String) -> CanonicalError {
    CanonicalError(
        "parsePackageCoordinate: '\(coordinate)' is not a valid Kanonak package coordinate (\(reason)); "
        + "expected publisher/package@major.minor.patch")
}

/// Parse a package's coordinate `publisher/package@version` into its parts.
/// Throws on malformed input, on a resource's coordinate (which has a
/// `/name`), and on a versionless one.
public func parsePackageCoordinate(_ coordinate: String) throws -> PackageCoordinate {
    if coordinate.isEmpty { throw invalidPackage(coordinate, "empty string") }
    if coordinate.unicodeScalars.contains(where: { CharacterSet.whitespacesAndNewlines.contains($0) }) {
        throw invalidPackage(coordinate, "whitespace is not allowed")
    }
    if coordinate.contains("#") {
        throw invalidPackage(coordinate, "'#' is reserved for embedded resources")
    }

    let segments = coordinate.split(separator: "/", omittingEmptySubsequences: false)
    guard segments.count == 2 else {
        throw invalidPackage(coordinate, "expected exactly 2 '/'-separated segments, got \(segments.count)")
    }
    let publisher = String(segments[0])
    let middle = String(segments[1])
    if publisher.isEmpty || middle.isEmpty {
        throw invalidPackage(coordinate, "empty segment")
    }
    if publisher.contains("@") {
        throw invalidPackage(coordinate, "'@' is only valid after the package name")
    }

    guard let at = middle.firstIndex(of: "@") else {
        throw invalidPackage(coordinate, "no version: a package coordinate states one")
    }
    let pkg = String(middle[..<at])
    let versionPart = middle[middle.index(after: at)...]
    if pkg.isEmpty { throw invalidPackage(coordinate, "empty package name before @") }
    if versionPart.contains("@") { throw invalidPackage(coordinate, "more than one '@'") }

    guard let version = parseVersion(versionPart) else {
        throw invalidPackage(coordinate, versionReason(versionPart))
    }
    return PackageCoordinate(publisher: publisher, package: pkg, version: version)
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

/// The TOTAL display accessor — for rendering a coordinate to a human (an
/// evaluation trace, an error message, a form label), where the string may
/// not be one the caller constructed and a throw is strictly worse than
/// showing the raw URI. A valid coordinate displays as its local name;
/// anything else is returned VERBATIM — never a best-effort tail, so a full
/// URI where a short name was expected is an honest signal of upstream
/// malformation. Display uses the STRICT grammar: only a full parse earns a
/// shortened name (an input the lenient carrier key would route still
/// displays verbatim).
public func displayName(_ uri: String) -> String {
    if let c = try? parseCoordinate(uri) {
        return c.name
    }
    return uri
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

/// Whether whether data whose terms were written at version `written` can be read by a reader whose schema is at version `reader` (runtime#28), pinned by readableVectors: a different major is never readable; from 1.0.0 the reader needs a minor at least the writer's (a patch changes no term, so it is not compared); below 1.0.0 the minor is the incompatible line and the reader needs the same minor and a patch at least the writer's.
public func isReadableBy(_ written: CoordinateVersion, _ reader: CoordinateVersion) -> Bool {
    if written.major != reader.major { return false }
    if written.major == 0 { return reader.minor == written.minor && reader.patch >= written.patch }
    return reader.minor >= written.minor
}
