/**
 * Kanonak coordinate parsing — `publisher/package[@version]/name` (issue #17).
 *
 * Two operations, two strictness levels, both pinned by
 * `vectors/coordinate-vectors.json`:
 *
 * - `parseCoordinate` (and the `versionlessKey` / `localName` conveniences) is
 *   STRICT: it throws on anything that is not a well-formed coordinate rather
 *   than returning a plausible-looking tail. This is the public API a consumer
 *   (a generated SDK binding, an evaluation trace, an error message) uses.
 * - `lenientVersionlessKey` is the total, non-throwing structural variant that
 *   carrier routing uses: it reduces a datatype URI to its versionless key
 *   WITHOUT validating the version content, or returns `undefined` when the
 *   input does not have the three-segment shape. It exists so `carrierOf`
 *   stays total and byte-identical under `canonicalFormVersion` "1" — an
 *   unparseable datatype URI routes to no carrier (raw token preserved),
 *   never to a guess and never to a crash.
 *
 * No ordering API on purpose: runtime consumers compare coordinates for
 * equality only. Version ranges, compatibility, and resolution live in the
 * SDK, not here. `#` is reserved for embedded resources and rejected by the
 * strict parser.
 */

export interface CoordinateVersion {
  major: number;
  minor: number;
  patch: number;
}

export interface Coordinate {
  publisher: string;
  package_: string;
  name: string;
  /** Absent for the versionless form (`publisher/package/name`). */
  version?: CoordinateVersion;
}

/** Exactly `0` or a digit run with no leading zero — one version part. */
const VERSION_PART = /^(0|[1-9]\d*)$/;

function invalid(uri: string, reason: string): Error {
  return new Error(
    `parseCoordinate: '${uri}' is not a valid Kanonak coordinate (${reason}); ` +
    `expected publisher/package[@major.minor.patch]/name`,
  );
}

/**
 * Parse a coordinate `publisher/package[@version]/name` into its parts.
 * Throws on malformed input — never returns a plausible-looking tail.
 */
export function parseCoordinate(uri: string): Coordinate {
  if (uri.length === 0) throw invalid(uri, 'empty string');
  if (/\s/.test(uri)) throw invalid(uri, 'whitespace is not allowed');
  if (uri.includes('#')) {
    throw invalid(uri, "'#' is reserved for embedded resources");
  }

  const segments = uri.split('/');
  if (segments.length !== 3) {
    throw invalid(uri, `expected exactly 3 '/'-separated segments, got ${segments.length}`);
  }
  const [publisher, middle, name] = segments;
  if (!publisher || !middle || !name) throw invalid(uri, 'empty segment');
  if (publisher.includes('@')) {
    throw invalid(uri, "'@' is only valid after the package name");
  }
  if (name.includes('@')) {
    throw invalid(uri, "'@' is only valid after the package name");
  }

  const at = middle.indexOf('@');
  if (at === -1) {
    return { publisher, package_: middle, name };
  }

  const package_ = middle.slice(0, at);
  const versionPart = middle.slice(at + 1);
  if (!package_) throw invalid(uri, 'empty package name before @');
  if (versionPart.includes('@')) throw invalid(uri, "more than one '@'");

  const parts = versionPart.split('.');
  if (parts.length !== 3 || !parts.every((p) => VERSION_PART.test(p))) {
    throw invalid(
      uri,
      `version '${versionPart}' is not exactly major.minor.patch (digits, no leading zeros)`,
    );
  }

  return {
    publisher,
    package_,
    name,
    version: {
      major: Number(parts[0]),
      minor: Number(parts[1]),
      patch: Number(parts[2]),
    },
  };
}

/**
 * The versionless canonical key `publisher/package/name` of a coordinate.
 * Strict: throws on malformed input.
 */
export function versionlessKey(uri: string): string {
  const c = parseCoordinate(uri);
  return `${c.publisher}/${c.package_}/${c.name}`;
}

/** The local name of a coordinate. Strict: throws on malformed input. */
export function localName(uri: string): string {
  return parseCoordinate(uri).name;
}

/**
 * The TOTAL display accessor — for rendering a coordinate to a human (an
 * evaluation trace, an error message, a form label), where the string may not
 * be one the caller constructed and a throw is strictly worse than showing
 * the raw URI. A valid coordinate displays as its local name; anything else
 * is returned VERBATIM — never a best-effort tail, so a full URI where a
 * short name was expected is an honest signal of upstream malformation.
 * Display uses the STRICT grammar: only a full parse earns a shortened name
 * (an input the lenient carrier key would route still displays verbatim).
 */
export function displayName(uri: string): string {
  try {
    return parseCoordinate(uri).name;
  } catch {
    return uri;
  }
}

/**
 * Total structural reduction to the versionless key, for carrier routing:
 * three non-empty `/`-segments required, everything from the first `@` in the
 * middle segment discarded WITHOUT validating it (the part before the `@`
 * must be non-empty). Returns `undefined` for anything else — never throws.
 */
export function lenientVersionlessKey(uri: string): string | undefined {
  const segments = uri.split('/');
  if (segments.length !== 3) return undefined;
  const [publisher, middle, name] = segments;
  if (!publisher || !middle || !name) return undefined;
  const at = middle.indexOf('@');
  const pkg = at === -1 ? middle : middle.slice(0, at);
  if (!pkg) return undefined;
  return `${publisher}/${pkg}/${name}`;
}
