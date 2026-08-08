using System;
using System.Text.RegularExpressions;

namespace Kanonak.Canonical
{
    /// <summary>
    /// One parsed coordinate version — exactly <c>major.minor.patch</c>, decimal
    /// digits with no leading zeros, no signs, no operators.
    /// </summary>
    public readonly struct CoordinateVersion
    {
        public int Major { get; }
        public int Minor { get; }
        public int Patch { get; }

        public CoordinateVersion(int major, int minor, int patch)
        {
            Major = major;
            Minor = minor;
            Patch = patch;
        }
    }

    /// <summary>
    /// Kanonak coordinate parsing — <c>publisher/package[@version]/name</c> (issue #17).
    ///
    /// Two operations, two strictness levels, both pinned by
    /// <c>vectors/coordinate-vectors.json</c>:
    /// <list type="bullet">
    /// <item><see cref="Parse"/> (and the <see cref="VersionlessKey"/> /
    /// <see cref="LocalName"/> conveniences) is STRICT: it throws on anything that
    /// is not a well-formed coordinate rather than returning a plausible-looking
    /// tail. This is the public API a consumer (a generated SDK binding, an
    /// evaluation trace, an error message) uses.</item>
    /// <item><see cref="LenientVersionlessKey"/> is the total, non-throwing
    /// structural variant that carrier routing uses: it reduces a datatype URI to
    /// its versionless key WITHOUT validating the version content, or returns
    /// <c>null</c> when the input does not have the three-segment shape. It exists
    /// so <see cref="CarrierMap.CarrierOf"/> stays total and byte-identical under
    /// canonicalFormVersion "1" — an unparseable datatype URI routes to no carrier
    /// (raw token preserved), never to a guess and never to a crash.</item>
    /// </list>
    ///
    /// No ordering API on purpose: runtime consumers compare coordinates for
    /// equality only. Version ranges, compatibility, and resolution live in the
    /// SDK, not here. <c>#</c> is reserved for embedded resources and rejected by
    /// the strict parser.
    /// </summary>
    public readonly struct Coordinate
    {
        public string Publisher { get; }
        public string Package { get; }
        public string Name { get; }
        /// <summary><c>null</c> for the versionless form (<c>publisher/package/name</c>).</summary>
        public CoordinateVersion? Version { get; }

        public Coordinate(string publisher, string package, string name, CoordinateVersion? version)
        {
            Publisher = publisher;
            Package = package;
            Name = name;
            Version = version;
        }

        /// <summary>Exactly <c>0</c> or a digit run with no leading zero — one version part.</summary>
        private static readonly Regex VersionPartRe = new Regex(@"^(0|[1-9]\d*)$", RegexOptions.Compiled);
        private static readonly Regex WhitespaceRe = new Regex(@"\s", RegexOptions.Compiled);

        private static FormatException Invalid(string uri, string reason)
        {
            return new FormatException(
                $"Coordinate.Parse: '{uri}' is not a valid Kanonak coordinate ({reason}); " +
                "expected publisher/package[@major.minor.patch]/name");
        }

        /// <summary>
        /// Parse a coordinate <c>publisher/package[@version]/name</c> into its parts.
        /// Throws on malformed input — never returns a plausible-looking tail.
        /// </summary>
        public static Coordinate Parse(string uri)
        {
            if (string.IsNullOrEmpty(uri)) throw Invalid(uri, "empty string");
            if (WhitespaceRe.IsMatch(uri)) throw Invalid(uri, "whitespace is not allowed");
            if (uri.IndexOf('#') >= 0) throw Invalid(uri, "'#' is reserved for embedded resources");

            string[] segments = uri.Split('/');
            if (segments.Length != 3)
                throw Invalid(uri, $"expected exactly 3 '/'-separated segments, got {segments.Length}");
            string publisher = segments[0], middle = segments[1], name = segments[2];
            if (publisher.Length == 0 || middle.Length == 0 || name.Length == 0)
                throw Invalid(uri, "empty segment");
            if (publisher.IndexOf('@') >= 0) throw Invalid(uri, "'@' is only valid after the package name");
            if (name.IndexOf('@') >= 0) throw Invalid(uri, "'@' is only valid after the package name");

            int at = middle.IndexOf('@');
            if (at < 0) return new Coordinate(publisher, middle, name, null);

            string package = middle.Substring(0, at);
            string versionPart = middle.Substring(at + 1);
            if (package.Length == 0) throw Invalid(uri, "empty package name before @");
            if (versionPart.IndexOf('@') >= 0) throw Invalid(uri, "more than one '@'");

            string[] parts = versionPart.Split('.');
            if (parts.Length != 3
                || !VersionPartRe.IsMatch(parts[0])
                || !VersionPartRe.IsMatch(parts[1])
                || !VersionPartRe.IsMatch(parts[2]))
            {
                throw Invalid(uri,
                    $"version '{versionPart}' is not exactly major.minor.patch (digits, no leading zeros)");
            }

            return new Coordinate(publisher, package, name,
                new CoordinateVersion(int.Parse(parts[0]), int.Parse(parts[1]), int.Parse(parts[2])));
        }

        /// <summary>
        /// The versionless canonical key <c>publisher/package/name</c> of a coordinate.
        /// Strict: throws on malformed input.
        /// </summary>
        public static string VersionlessKey(string uri)
        {
            Coordinate c = Parse(uri);
            return c.Publisher + "/" + c.Package + "/" + c.Name;
        }

        /// <summary>The local name of a coordinate. Strict: throws on malformed input.</summary>
        public static string LocalName(string uri)
        {
            return Parse(uri).Name;
        }

        /// <summary>
        /// Total structural reduction to the versionless key, for carrier routing:
        /// three non-empty <c>/</c>-segments required, everything from the first
        /// <c>@</c> in the middle segment discarded WITHOUT validating it (the part
        /// before the <c>@</c> must be non-empty). Returns <c>null</c> for anything
        /// else — never throws.
        /// </summary>
        public static string LenientVersionlessKey(string uri)
        {
            string[] segments = uri.Split('/');
            if (segments.Length != 3) return null;
            string publisher = segments[0], middle = segments[1], name = segments[2];
            if (publisher.Length == 0 || middle.Length == 0 || name.Length == 0) return null;
            int at = middle.IndexOf('@');
            string package = at < 0 ? middle : middle.Substring(0, at);
            if (package.Length == 0) return null;
            return publisher + "/" + package + "/" + name;
        }
    }
}
