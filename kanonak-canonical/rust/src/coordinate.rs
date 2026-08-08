//! Kanonak coordinate parsing — `publisher/package[@version]/name` (issue #17).
//!
//! Two operations, two strictness levels, both pinned by
//! `vectors/coordinate-vectors.json`:
//!
//! - `parse_coordinate` (and the `versionless_key` / `local_name`
//!   conveniences) is STRICT: it errors on anything that is not a well-formed
//!   coordinate rather than returning a plausible-looking tail. This is the
//!   public API a consumer (a generated SDK binding, an evaluation trace, an
//!   error message) uses.
//! - `lenient_versionless_key` is the total, non-erroring structural variant
//!   that carrier routing uses: it reduces a datatype URI to its versionless
//!   key WITHOUT validating the version content, or returns `None` when the
//!   input does not have the three-segment shape. It exists so `carrier_of`
//!   stays total and byte-identical under `CANONICAL_FORM_VERSION` "1" — an
//!   unparseable datatype URI routes to no carrier (raw token preserved),
//!   never to a guess and never to a crash.
//!
//! No ordering API on purpose: runtime consumers compare coordinates for
//! equality only. Version ranges, compatibility, and resolution live in the
//! SDK, not here. `#` is reserved for embedded resources and rejected by the
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

    let parts: Vec<&str> = version_str.split('.').collect();
    let nums: Option<Vec<u64>> = if parts.len() == 3 {
        parts.iter().map(|p| version_part(p)).collect()
    } else {
        None
    };
    match nums {
        Some(n) => Ok(Coordinate {
            publisher: publisher.to_string(),
            package: package.to_string(),
            name: name.to_string(),
            version: Some(CoordinateVersion {
                major: n[0],
                minor: n[1],
                patch: n[2],
            }),
        }),
        None => Err(invalid(
            uri,
            &format!(
                "version '{}' is not exactly major.minor.patch (digits, no leading zeros)",
                version_str
            ),
        )),
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
