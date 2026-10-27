//! Kanonak coordinate parsing — `publisher/package[@version]/name` (issue #17),
//! and a package's own coordinate, `publisher/package@version` (runtime#53).
//!
//! Two operations, two strictness levels, both pinned by
//! `vectors/coordinate-vectors.json`:
//!
//! - `parse_coordinate` (and the `versionless_key` / `local_name`
//!   conveniences) is STRICT: it errors on anything that is not a well-formed
//!   coordinate rather than returning a plausible-looking tail. This is the
//!   public API a consumer (a generated SDK binding, an evaluation trace, an
//!   error message) uses.
//! - `parse_package_coordinate` is the same STRICT grammar for a package at a
//!   version, as a lock names one: no `/name`, and the version required.
//! - `lenient_versionless_key` is the total, non-erroring structural variant
//!   that carrier routing uses: it reduces a datatype URI to its versionless
//!   key WITHOUT validating the version content, or returns `None` when the
//!   input does not have the three-segment shape. It exists so `carrier_of`
//!   stays total and byte-identical under `CANONICAL_FORM_VERSION` "1" — an
//!   unparseable datatype URI routes to no carrier (raw token preserved),
//!   never to a guess and never to a crash.
//!
//! No ordering API on purpose: runtime consumers compare coordinates for
//! equality, plus ONE version relation (runtime#28): `is_readable_by(written,
//! reader)` — whether data written at one version can be read by a schema at
//! another. Import ranges and resolution live in the SDK, not here. `#` is reserved for embedded resources and rejected by the
//! strict parser.

use crate::CanonError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CoordinateVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Coordinate {
    pub publisher: String,
    pub package: String,
    pub name: String,
    /// `None` for the versionless form (`publisher/package/name`).
    pub version: Option<CoordinateVersion>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PackageCoordinate {
    pub publisher: String,
    pub package: String,
    pub version: CoordinateVersion,
}

fn invalid(uri: &str, reason: &str) -> CanonError {
    CanonError(format!(
        "parseCoordinate: '{}' is not a valid Kanonak coordinate ({}); \
         expected publisher/package[@major.minor.patch]/name",
        uri, reason
    ))
}

/// Exactly `0` or a digit run with no leading zero — one version part.
fn version_part(p: &str) -> Option<u64> {
    if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if p.len() > 1 && p.starts_with('0') {
        return None;
    }
    p.parse().ok()
}

/// `major.minor.patch`, each part a `version_part`, or `None`.
fn parse_version(text: &str) -> Option<CoordinateVersion> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some(CoordinateVersion {
        major: version_part(parts[0])?,
        minor: version_part(parts[1])?,
        patch: version_part(parts[2])?,
    })
}

fn version_reason(text: &str) -> String {
    format!(
        "version '{}' is not exactly major.minor.patch (digits, no leading zeros)",
        text
    )
}

/// Parse a coordinate `publisher/package[@version]/name` into its parts.
/// Errors on malformed input — never returns a plausible-looking tail.
pub fn parse_coordinate(uri: &str) -> Result<Coordinate, CanonError> {
    if uri.is_empty() {
        return Err(invalid(uri, "empty string"));
    }
    if uri.chars().any(char::is_whitespace) {
        return Err(invalid(uri, "whitespace is not allowed"));
    }
    if uri.contains('#') {
        return Err(invalid(uri, "'#' is reserved for embedded resources"));
    }

    let segments: Vec<&str> = uri.split('/').collect();
    if segments.len() != 3 {
        return Err(invalid(
            uri,
            &format!(
                "expected exactly 3 '/'-separated segments, got {}",
                segments.len()
            ),
        ));
    }
    let (publisher, middle, name) = (segments[0], segments[1], segments[2]);
    if publisher.is_empty() || middle.is_empty() || name.is_empty() {
        return Err(invalid(uri, "empty segment"));
    }
    if publisher.contains('@') || name.contains('@') {
        return Err(invalid(uri, "'@' is only valid after the package name"));
    }

    let at = match middle.find('@') {
        None => {
            return Ok(Coordinate {
                publisher: publisher.to_string(),
                package: middle.to_string(),
                name: name.to_string(),
                version: None,
            })
        }
        Some(i) => i,
    };

    let package = &middle[..at];
    let version_str = &middle[at + 1..];
    if package.is_empty() {
        return Err(invalid(uri, "empty package name before @"));
    }
    if version_str.contains('@') {
        return Err(invalid(uri, "more than one '@'"));
    }

    match parse_version(version_str) {
        Some(version) => Ok(Coordinate {
            publisher: publisher.to_string(),
            package: package.to_string(),
            name: name.to_string(),
            version: Some(version),
        }),
        None => Err(invalid(uri, &version_reason(version_str))),
    }
}

fn invalid_package(coordinate: &str, reason: &str) -> CanonError {
    CanonError(format!(
        "parsePackageCoordinate: '{}' is not a valid Kanonak package coordinate ({}); \
         expected publisher/package@major.minor.patch",
        coordinate, reason
    ))
}

/// Parse a package's coordinate `publisher/package@version` into its parts.
/// Errors on malformed input, on a resource's coordinate (which has a
/// `/name`), and on a versionless one.
pub fn parse_package_coordinate(coordinate: &str) -> Result<PackageCoordinate, CanonError> {
    if coordinate.is_empty() {
        return Err(invalid_package(coordinate, "empty string"));
    }
    if coordinate.chars().any(char::is_whitespace) {
        return Err(invalid_package(coordinate, "whitespace is not allowed"));
    }
    if coordinate.contains('#') {
        return Err(invalid_package(
            coordinate,
            "'#' is reserved for embedded resources",
        ));
    }

    let segments: Vec<&str> = coordinate.split('/').collect();
    if segments.len() != 2 {
        return Err(invalid_package(
            coordinate,
            &format!(
                "expected exactly 2 '/'-separated segments, got {}",
                segments.len()
            ),
        ));
    }
    let (publisher, middle) = (segments[0], segments[1]);
    if publisher.is_empty() || middle.is_empty() {
        return Err(invalid_package(coordinate, "empty segment"));
    }
    if publisher.contains('@') {
        return Err(invalid_package(
            coordinate,
            "'@' is only valid after the package name",
        ));
    }

    let at = match middle.find('@') {
        None => {
            return Err(invalid_package(
                coordinate,
                "no version: a package coordinate states one",
            ))
        }
        Some(i) => i,
    };
    let package = &middle[..at];
    let version_str = &middle[at + 1..];
    if package.is_empty() {
        return Err(invalid_package(coordinate, "empty package name before @"));
    }
    if version_str.contains('@') {
        return Err(invalid_package(coordinate, "more than one '@'"));
    }

    match parse_version(version_str) {
        Some(version) => Ok(PackageCoordinate {
            publisher: publisher.to_string(),
            package: package.to_string(),
            version,
        }),
        None => Err(invalid_package(coordinate, &version_reason(version_str))),
    }
}

/// The versionless canonical key `publisher/package/name` of a coordinate.
/// Strict: errors on malformed input.
pub fn versionless_key(uri: &str) -> Result<String, CanonError> {
    let c = parse_coordinate(uri)?;
    Ok(format!("{}/{}/{}", c.publisher, c.package, c.name))
}

/// The local name of a coordinate. Strict: errors on malformed input.
pub fn local_name(uri: &str) -> Result<String, CanonError> {
    Ok(parse_coordinate(uri)?.name)
}

/// The TOTAL display accessor — for rendering a coordinate to a human (an
/// evaluation trace, an error message, a form label), where the string may
/// not be one the caller constructed and an error is strictly worse than
/// showing the raw URI. A valid coordinate displays as its local name;
/// anything else is returned VERBATIM — never a best-effort tail, so a full
/// URI where a short name was expected is an honest signal of upstream
/// malformation. Display uses the STRICT grammar: only a full parse earns a
/// shortened name (an input the lenient carrier key would route still
/// displays verbatim).
pub fn display_name(uri: &str) -> String {
    match parse_coordinate(uri) {
        Ok(c) => c.name,
        Err(_) => uri.to_string(),
    }
}

/// Total structural reduction to the versionless key, for carrier routing:
/// three non-empty `/`-segments required, everything from the first `@` in the
/// middle segment discarded WITHOUT validating it (the part before the `@`
/// must be non-empty). Returns `None` for anything else — never panics.
pub fn lenient_versionless_key(uri: &str) -> Option<String> {
    let mut it = uri.split('/');
    let (publisher, middle, name) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    if publisher.is_empty() || middle.is_empty() || name.is_empty() {
        return None;
    }
    let pkg = match middle.find('@') {
        Some(i) => &middle[..i],
        None => middle,
    };
    if pkg.is_empty() {
        return None;
    }
    Some(format!("{}/{}/{}", publisher, pkg, name))
}

/// Whether data whose terms were written at version `written` can be read by a
/// reader whose schema is at version `reader` (runtime#28), pinned by
/// `readableVectors`: a different major is never readable; from 1.0.0 the
/// reader needs a minor at least the writer's (a patch changes no term, so it
/// is not compared); below 1.0.0 the minor is the incompatible line and the
/// reader needs the same minor and a patch at least the writer's.
pub fn is_readable_by(written: CoordinateVersion, reader: CoordinateVersion) -> bool {
    if written.major != reader.major {
        return false;
    }
    if written.major == 0 {
        return reader.minor == written.minor && reader.patch >= written.patch;
    }
    reader.minor >= written.minor
}
