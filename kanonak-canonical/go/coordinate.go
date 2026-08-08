// Kanonak coordinate parsing — publisher/package[@version]/name (issue #17).
//
// Two operations, two strictness levels, both pinned by
// vectors/coordinate-vectors.json:
//
//   - ParseCoordinate (and the VersionlessKey / LocalName conveniences) is
//     STRICT: it errors on anything that is not a well-formed coordinate
//     rather than returning a plausible-looking tail. This is the public API
//     a consumer (a generated SDK binding, an evaluation trace, an error
//     message) uses.
//   - LenientVersionlessKey is the total, non-erroring structural variant
//     that carrier routing uses: it reduces a datatype URI to its versionless
//     key WITHOUT validating the version content, or returns ok=false when
//     the input does not have the three-segment shape. It exists so CarrierOf
//     stays total and byte-identical under canonicalFormVersion "1" — an
//     unparseable datatype URI routes to no carrier (raw token preserved),
//     never to a guess and never to a crash.
//
// No ordering API on purpose: runtime consumers compare coordinates for
// equality only. Version ranges, compatibility, and resolution live in the
// SDK, not here. '#' is reserved for embedded resources and rejected by the
// strict parser.
package canonical

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"
	"unicode"
)

type CoordinateVersion struct {
	Major int
	Minor int
	Patch int
}

type Coordinate struct {
	Publisher string
	Package   string
	Name      string
	// Version is nil for the versionless form (publisher/package/name).
	Version *CoordinateVersion
}

// Exactly 0 or a digit run with no leading zero — one version part.
var versionPartRe = regexp.MustCompile(`^(0|[1-9][0-9]*)$`)

func invalidCoordinate(uri, reason string) error {
	return fmt.Errorf(
		"parseCoordinate: %q is not a valid Kanonak coordinate (%s); expected publisher/package[@major.minor.patch]/name",
		uri, reason)
}

// ParseCoordinate parses publisher/package[@version]/name into its parts.
// Errors on malformed input — never returns a plausible-looking tail.
func ParseCoordinate(uri string) (Coordinate, error) {
	if uri == "" {
		return Coordinate{}, invalidCoordinate(uri, "empty string")
	}
	if strings.IndexFunc(uri, unicode.IsSpace) >= 0 {
		return Coordinate{}, invalidCoordinate(uri, "whitespace is not allowed")
	}
	if strings.Contains(uri, "#") {
		return Coordinate{}, invalidCoordinate(uri, "'#' is reserved for embedded resources")
	}

	segments := strings.Split(uri, "/")
	if len(segments) != 3 {
		return Coordinate{}, invalidCoordinate(uri,
			fmt.Sprintf("expected exactly 3 '/'-separated segments, got %d", len(segments)))
	}
	publisher, middle, name := segments[0], segments[1], segments[2]
	if publisher == "" || middle == "" || name == "" {
		return Coordinate{}, invalidCoordinate(uri, "empty segment")
	}
	if strings.Contains(publisher, "@") || strings.Contains(name, "@") {
		return Coordinate{}, invalidCoordinate(uri, "'@' is only valid after the package name")
	}

	at := strings.Index(middle, "@")
	if at == -1 {
		return Coordinate{Publisher: publisher, Package: middle, Name: name}, nil
	}

	pkg := middle[:at]
	versionPart := middle[at+1:]
	if pkg == "" {
		return Coordinate{}, invalidCoordinate(uri, "empty package name before @")
	}
	if strings.Contains(versionPart, "@") {
		return Coordinate{}, invalidCoordinate(uri, "more than one '@'")
	}

	parts := strings.Split(versionPart, ".")
	if len(parts) != 3 || !versionPartRe.MatchString(parts[0]) ||
		!versionPartRe.MatchString(parts[1]) || !versionPartRe.MatchString(parts[2]) {
		return Coordinate{}, invalidCoordinate(uri,
			fmt.Sprintf("version %q is not exactly major.minor.patch (digits, no leading zeros)", versionPart))
	}

	major, _ := strconv.Atoi(parts[0])
	minor, _ := strconv.Atoi(parts[1])
	patch, _ := strconv.Atoi(parts[2])
	return Coordinate{
		Publisher: publisher,
		Package:   pkg,
		Name:      name,
		Version:   &CoordinateVersion{Major: major, Minor: minor, Patch: patch},
	}, nil
}

// VersionlessKey returns the versionless canonical key publisher/package/name
// of a coordinate. Strict: errors on malformed input.
func VersionlessKey(uri string) (string, error) {
	c, err := ParseCoordinate(uri)
	if err != nil {
		return "", err
	}
	return c.Publisher + "/" + c.Package + "/" + c.Name, nil
}

// LocalName returns the local name of a coordinate. Strict: errors on
// malformed input.
func LocalName(uri string) (string, error) {
	c, err := ParseCoordinate(uri)
	if err != nil {
		return "", err
	}
	return c.Name, nil
}

// DisplayName is the TOTAL display accessor — for rendering a coordinate to a
// human (an evaluation trace, an error message, a form label), where the
// string may not be one the caller constructed and an error is strictly worse
// than showing the raw URI. A valid coordinate displays as its local name;
// anything else is returned VERBATIM — never a best-effort tail, so a full
// URI where a short name was expected is an honest signal of upstream
// malformation. Display uses the STRICT grammar: only a full parse earns a
// shortened name (an input the lenient carrier key would route still
// displays verbatim).
func DisplayName(uri string) string {
	c, err := ParseCoordinate(uri)
	if err != nil {
		return uri
	}
	return c.Name
}

// LenientVersionlessKey is the total structural reduction to the versionless
// key, for carrier routing: three non-empty '/'-segments required, everything
// from the first '@' in the middle segment discarded WITHOUT validating it
// (the part before the '@' must be non-empty). Returns ok=false for anything
// else — never errors.
func LenientVersionlessKey(uri string) (string, bool) {
	segments := strings.Split(uri, "/")
	if len(segments) != 3 {
		return "", false
	}
	publisher, middle, name := segments[0], segments[1], segments[2]
	if publisher == "" || middle == "" || name == "" {
		return "", false
	}
	pkg := middle
	if at := strings.Index(middle, "@"); at != -1 {
		pkg = middle[:at]
	}
	if pkg == "" {
		return "", false
	}
	return publisher + "/" + pkg + "/" + name, true
}
