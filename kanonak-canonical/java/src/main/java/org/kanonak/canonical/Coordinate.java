package org.kanonak.canonical;

import java.util.regex.Pattern;

/**
 * Kanonak coordinate parsing — {@code publisher/package[@version]/name}
 * (issue #17). Two operations, two strictness levels, both pinned by
 * {@code vectors/coordinate-vectors.json}:
 *
 * <ul>
 * <li>{@link #parse} (and the {@link #versionlessKey} / {@link #localName}
 *     conveniences) is STRICT: it throws on anything that is not a well-formed
 *     coordinate rather than returning a plausible-looking tail. This is the
 *     public API a consumer (a generated SDK binding, an evaluation trace, an
 *     error message) uses.</li>
 * <li>{@link #lenientVersionlessKey} is the total, non-throwing structural
 *     variant that carrier routing uses: it reduces a datatype URI to its
 *     versionless key WITHOUT validating the version content, or returns
 *     {@code null} when the input does not have the three-segment shape. It
 *     exists so {@link Carrier#of} stays total and byte-identical under
 *     canonicalFormVersion "1" — an unparseable datatype URI routes to no
 *     carrier (raw token preserved), never to a guess and never to a crash.</li>
 * </ul>
 *
 * No ordering API on purpose: runtime consumers compare coordinates for
 * equality only. Version ranges, compatibility, and resolution live in the
 * SDK, not here. {@code '#'} is reserved for embedded resources and rejected
 * by the strict parser.
 */
public record Coordinate(String publisher, String packageName, String name, Version version) {

    /** An exact version {@code major.minor.patch}; {@code null} on the coordinate means the versionless form. */
    public record Version(int major, int minor, int patch) {}

    /** Exactly {@code 0} or a digit run with no leading zero — one version part. */
    private static final Pattern VERSION_PART = Pattern.compile("0|[1-9][0-9]*");

    private static final Pattern WHITESPACE = Pattern.compile("\\s");

    private static IllegalArgumentException invalid(String uri, String reason) {
        return new IllegalArgumentException(
            "parseCoordinate: '" + uri + "' is not a valid Kanonak coordinate (" + reason + "); "
            + "expected publisher/package[@major.minor.patch]/name");
    }

    /**
     * Parse a coordinate {@code publisher/package[@version]/name} into its parts.
     * Throws {@link IllegalArgumentException} on malformed input — never returns
     * a plausible-looking tail.
     */
    public static Coordinate parse(String uri) {
        if (uri.isEmpty()) throw invalid(uri, "empty string");
        if (WHITESPACE.matcher(uri).find()) throw invalid(uri, "whitespace is not allowed");
        if (uri.indexOf('#') >= 0) throw invalid(uri, "'#' is reserved for embedded resources");

        String[] segments = uri.split("/", -1);
        if (segments.length != 3)
            throw invalid(uri, "expected exactly 3 '/'-separated segments, got " + segments.length);
        String publisher = segments[0], middle = segments[1], name = segments[2];
        if (publisher.isEmpty() || middle.isEmpty() || name.isEmpty()) throw invalid(uri, "empty segment");
        if (publisher.indexOf('@') >= 0) throw invalid(uri, "'@' is only valid after the package name");
        if (name.indexOf('@') >= 0) throw invalid(uri, "'@' is only valid after the package name");

        int at = middle.indexOf('@');
        if (at == -1) return new Coordinate(publisher, middle, name, null);

        String packageName = middle.substring(0, at);
        String versionPart = middle.substring(at + 1);
        if (packageName.isEmpty()) throw invalid(uri, "empty package name before @");
        if (versionPart.indexOf('@') >= 0) throw invalid(uri, "more than one '@'");

        String[] parts = versionPart.split("\\.", -1);
        boolean ok = parts.length == 3;
        for (int i = 0; ok && i < 3; i++) ok = VERSION_PART.matcher(parts[i]).matches();
        if (!ok)
            throw invalid(uri, "version '" + versionPart + "' is not exactly major.minor.patch (digits, no leading zeros)");

        return new Coordinate(publisher, packageName, name,
            new Version(Integer.parseInt(parts[0]), Integer.parseInt(parts[1]), Integer.parseInt(parts[2])));
    }

    /**
     * The versionless canonical key {@code publisher/package/name} of a coordinate.
     * Strict: throws on malformed input.
     */
    public static String versionlessKey(String uri) {
        Coordinate c = parse(uri);
        return c.publisher + "/" + c.packageName + "/" + c.name;
    }

    /** The local name of a coordinate. Strict: throws on malformed input. */
    public static String localName(String uri) {
        return parse(uri).name;
    }

    /**
     * The TOTAL display accessor — for rendering a coordinate to a human (an
     * evaluation trace, an error message, a form label), where the string may
     * not be one the caller constructed and a throw is strictly worse than
     * showing the raw URI. A valid coordinate displays as its local name;
     * anything else is returned VERBATIM — never a best-effort tail, so a
     * full URI where a short name was expected is an honest signal of
     * upstream malformation. Display uses the STRICT grammar: only a full
     * parse earns a shortened name (an input the lenient carrier key would
     * route still displays verbatim).
     */
    public static String displayName(String uri) {
        try {
            return parse(uri).name;
        } catch (IllegalArgumentException e) {
            return uri;
        }
    }

    /**
     * Total structural reduction to the versionless key, for carrier routing:
     * three non-empty {@code '/'}-segments required, everything from the first
     * {@code '@'} in the middle segment discarded WITHOUT validating it (the
     * part before the {@code '@'} must be non-empty). Returns {@code null} for
     * anything else — never throws.
     */
    public static String lenientVersionlessKey(String uri) {
        String[] segments = uri.split("/", -1);
        if (segments.length != 3) return null;
        String publisher = segments[0], middle = segments[1], name = segments[2];
        if (publisher.isEmpty() || middle.isEmpty() || name.isEmpty()) return null;
        int at = middle.indexOf('@');
        String pkg = at == -1 ? middle : middle.substring(0, at);
        if (pkg.isEmpty()) return null;
        return publisher + "/" + pkg + "/" + name;
    }
}
