using System;
using System.Collections.Generic;

namespace Kanonak.Canonical
{
    /// <summary>
    /// The closed set of canonical-form carriers (canonicalFormVersion "1"). A
    /// datatype maps to exactly one carrier; the carrier determines both the
    /// canonical lexical rule and the tag that participates in identity.
    /// </summary>
    public enum Carrier
    {
        Integer,
        Decimal,
        Double,
        Float,
        Boolean,
        String,
        AnyUri,
        LangString,
        DateTime,
        Date,
        Time,
        HexBinary,
        Base64Binary,
    }

    /// <summary>Carrier wire tags — the exact strings that appear in the canonical form.</summary>
    public static class Carriers
    {
        public static string Tag(this Carrier c)
        {
            switch (c)
            {
                case Carrier.Integer: return "integer";
                case Carrier.Decimal: return "decimal";
                case Carrier.Double: return "double";
                case Carrier.Float: return "float";
                case Carrier.Boolean: return "boolean";
                case Carrier.String: return "string";
                case Carrier.AnyUri: return "anyURI";
                case Carrier.LangString: return "langString";
                case Carrier.DateTime: return "dateTime";
                case Carrier.Date: return "date";
                case Carrier.Time: return "time";
                case Carrier.HexBinary: return "hexBinary";
                case Carrier.Base64Binary: return "base64Binary";
                default: throw new ArgumentOutOfRangeException(nameof(c));
            }
        }
    }

    /// <summary>
    /// A datatype's durable identity (publisher / package / name), used to route
    /// to a carrier. Mirrors the SDK's <c>EntityUri</c> shape.
    /// </summary>
    public readonly struct EntityUri
    {
        public string Publisher { get; }
        public string Package { get; }
        public string Name { get; }

        public EntityUri(string publisher, string package, string name)
        {
            Publisher = publisher;
            Package = package;
            Name = name;
        }

        /// <summary>The carrier-lookup key: <c>publisher/package/name</c> (version-independent).</summary>
        public string Key => Publisher + "/" + Package + "/" + Name;

        /// <summary>
        /// Parse <c>publisher/package@ver/name</c> or <c>publisher/package/name</c>.
        /// Mirrors the vector decoder's <c>entityUri</c>.
        /// </summary>
        public static EntityUri Parse(string uri)
        {
            int idx = uri.LastIndexOf('/');
            string name = uri.Substring(idx + 1);
            string head = uri.Substring(0, idx); // publisher/package@ver
            int slash = head.IndexOf('/');
            string publisher = head.Substring(0, slash);
            string pkg = head.Substring(slash + 1);
            int at = pkg.IndexOf('@');
            if (at >= 0) pkg = pkg.Substring(0, at);
            return new EntityUri(publisher, pkg, name);
        }
    }

    /// <summary>
    /// Normative datatype-URI → carrier routing. Authored, not derived from the
    /// graph: the XSD value-space tree is a fixed W3C standard, so the routing is a
    /// hardcoded URI contract. The entire integer-derivation tree routes to
    /// <see cref="Carrier.Integer"/>; <c>normalizedString</c>/<c>token</c> route to
    /// <see cref="Carrier.String"/> (their whitespace behaviour is a facet, not value
    /// canonicalization). <c>rdf:langString</c> lives in core-rdf, not core-xsd.
    /// </summary>
    public static class CarrierMap
    {
        private static readonly Dictionary<string, Carrier> ByUri = BuildMap();
        private static readonly string LangStringKey = "kanonak.org/core-rdf/langString";

        private static Dictionary<string, Carrier> BuildMap()
        {
            var xsd = new (string Name, Carrier Carrier)[]
            {
                // Integer value space — base type + every derived/restricted integer.
                ("integer", Carrier.Integer),
                ("long", Carrier.Integer), ("int", Carrier.Integer), ("short", Carrier.Integer), ("byte", Carrier.Integer),
                ("unsignedLong", Carrier.Integer), ("unsignedInt", Carrier.Integer),
                ("unsignedShort", Carrier.Integer), ("unsignedByte", Carrier.Integer),
                ("nonNegativeInteger", Carrier.Integer), ("positiveInteger", Carrier.Integer),
                ("nonPositiveInteger", Carrier.Integer), ("negativeInteger", Carrier.Integer),
                // Decimal value space.
                ("decimal", Carrier.Decimal),
                // IEEE binary floating point — distinct carriers.
                ("double", Carrier.Double),
                ("float", Carrier.Float),
                // Boolean.
                ("boolean", Carrier.Boolean),
                // Strings — string + restricted string facets + anyURI.
                ("string", Carrier.String),
                ("normalizedString", Carrier.String),
                ("token", Carrier.String),
                ("anyURI", Carrier.AnyUri),
                // Temporal.
                ("dateTime", Carrier.DateTime),
                ("date", Carrier.Date),
                ("time", Carrier.Time),
                // Binary.
                ("hexBinary", Carrier.HexBinary),
                ("base64Binary", Carrier.Base64Binary),
            };
            var map = new Dictionary<string, Carrier>(xsd.Length);
            foreach (var (name, carrier) in xsd)
                map["kanonak.org/core-xsd/" + name] = carrier;
            return map;
        }

        /// <summary>
        /// The carrier for a datatype, or <c>null</c> if outside the v1 canonicalized
        /// set — an out-of-set datatype is canonicalized as a byte-preserved raw token
        /// (the untyped/open-world tier), never guessed into a carrier.
        /// </summary>
        public static Carrier? CarrierOf(EntityUri datatype)
        {
            string key = datatype.Key;
            if (key == LangStringKey) return Carrier.LangString;
            return ByUri.TryGetValue(key, out var c) ? c : (Carrier?)null;
        }
    }
}
