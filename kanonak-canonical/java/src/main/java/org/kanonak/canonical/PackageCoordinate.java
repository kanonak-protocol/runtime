package org.kanonak.canonical;

/**
 * A package's own coordinate, {@code publisher/package@version}, as a lock
 * names a package at a version (runtime#53). The same STRICT grammar as
 * {@link Coordinate#parse}, with no {@code /name} and the version required,
 * pinned by {@code packageParseVectors} in {@code vectors/coordinate-vectors.json}.
 */
public record PackageCoordinate(String publisher, String packageName, Coordinate.Version version) {

    private static IllegalArgumentException invalid(String coordinate, String reason) {
        return new IllegalArgumentException(
            "parsePackageCoordinate: '" + coordinate + "' is not a valid Kanonak package coordinate (" + reason + "); "
            + "expected publisher/package@major.minor.patch");
    }

    /**
     * Parse a package's coordinate {@code publisher/package@version} into its
     * parts. Throws {@link IllegalArgumentException} on malformed input, on a
     * resource's coordinate (which has a {@code /name}), and on a versionless one.
     */
    public static PackageCoordinate parse(String coordinate) {
        if (coordinate.isEmpty()) throw invalid(coordinate, "empty string");
        if (Coordinate.WHITESPACE.matcher(coordinate).find()) throw invalid(coordinate, "whitespace is not allowed");
        if (coordinate.indexOf('#') >= 0) throw invalid(coordinate, "'#' is reserved for embedded resources");

        String[] segments = coordinate.split("/", -1);
        if (segments.length != 2)
            throw invalid(coordinate, "expected exactly 2 '/'-separated segments, got " + segments.length);
        String publisher = segments[0], middle = segments[1];
        if (publisher.isEmpty() || middle.isEmpty()) throw invalid(coordinate, "empty segment");
        if (publisher.indexOf('@') >= 0) throw invalid(coordinate, "'@' is only valid after the package name");

        int at = middle.indexOf('@');
        if (at == -1) throw invalid(coordinate, "no version: a package coordinate states one");
        String packageName = middle.substring(0, at);
        String versionPart = middle.substring(at + 1);
        if (packageName.isEmpty()) throw invalid(coordinate, "empty package name before @");
        if (versionPart.indexOf('@') >= 0) throw invalid(coordinate, "more than one '@'");

        Coordinate.Version version = Coordinate.parseVersion(versionPart);
        if (version == null) throw invalid(coordinate, Coordinate.versionReason(versionPart));
        return new PackageCoordinate(publisher, packageName, version);
    }
}
