using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using Kanonak.Canonical;

namespace Kanonak.Codec
{
    /// <summary>
    /// The generic, ontology-independent codec runtime (C# port). Given a
    /// <see cref="CodecSchema"/> (the per-package metadata a generated SDK embeds)
    /// and a set of typed nodes, it builds the canonical input model and
    /// content-addresses it via <c>Kanonak.Canonical</c> — the same content form the
    /// reference ports and the <c>kanonak hash</c> CLI produce. It also (de)serializes
    /// the normalized-JSON wire form.
    ///
    /// A node is a plain map (the <c>$</c>-envelope plus alias-collapsed local-name
    /// fields). Field values are CLR primitives (<see cref="string"/>, <see cref="bool"/>,
    /// numeric), an <see cref="IReadOnlyList{T}"/> of those, or a reference map
    /// (<c>{ "$ref": uri }</c>). <c>$extra</c> is a map keyed by predicate URI.
    /// </summary>
    public static class Codec
    {
        /// <summary>Reserved <c>$</c>-envelope keys — never emitted as ontology statements.</summary>
        private static readonly HashSet<string> EnvelopeKeys = new HashSet<string>
        {
            "$type", "$id", "$contentHash", "$version", "$extra",
        };

        // -- Hashing / canonical form -------------------------------------------

        /// <summary>
        /// Build the canonical input model: a subject per node + the synthesized
        /// package-wrapper subject (raw label + <c>Package</c> type), exactly the
        /// subject set <c>kanonak hash</c> produces for the equivalent authored package.
        /// </summary>
        public static Package BuildPackage(
            IReadOnlyList<IReadOnlyDictionary<string, object>> nodes,
            CodecSchema schema,
            PackageContext pkg)
        {
            var subjects = new List<Subject>();
            foreach (var node in nodes)
            {
                string id = GetString(node, "$id");
                if (string.IsNullOrEmpty(id)) throw new ArgumentException("node is missing $id");
                subjects.Add(new Subject(id, StatementsFor(node, schema)));
            }

            string pkgUri = pkg.Publisher + "/" + pkg.PackageName + "@" + pkg.Version + "/" + pkg.PackageName;
            var pkgStatements = new List<Statement>();
            if (pkg.Label != null)
                pkgStatements.Add(new Statement(schema.LabelPredicate, new RawScalar(pkg.Label)));
            pkgStatements.Add(new Statement(schema.TypePredicate, new Reference(schema.PackageTypeUri)));
            subjects.Add(new Subject(pkgUri, pkgStatements));

            return new Package(subjects);
        }

        /// <summary>The canonical form (the <c>{subjects:[…]}</c> JSON) of a package built from nodes.</summary>
        public static string CanonicalForm(
            IReadOnlyList<IReadOnlyDictionary<string, object>> nodes,
            CodecSchema schema,
            PackageContext pkg)
            => Kanonak.Canonical.CanonicalForm.Serialize(BuildPackage(nodes, schema, pkg));

        /// <summary>The <c>sha256:</c> content hash of a package built from nodes — matches <c>kanonak hash</c>.</summary>
        public static string ContentHash(
            IReadOnlyList<IReadOnlyDictionary<string, object>> nodes,
            CodecSchema schema,
            PackageContext pkg)
            => Kanonak.Canonical.CanonicalForm.Hash(BuildPackage(nodes, schema, pkg));

        private static List<Statement> StatementsFor(IReadOnlyDictionary<string, object> node, CodecSchema schema)
        {
            string typeUri = GetString(node, "$type");
            if (string.IsNullOrEmpty(typeUri)) throw new ArgumentException("node is missing $type");
            if (!schema.Classes.TryGetValue(typeUri, out var cls))
                throw new ArgumentException("no schema for type " + typeUri);

            var statements = new List<Statement>
            {
                // The rdf:type triple every resource carries.
                new Statement(schema.TypePredicate, new Reference(typeUri)),
            };

            foreach (var kv in node)
            {
                string key = kv.Key;
                object raw = kv.Value;
                if (EnvelopeKeys.Contains(key) || raw == null) continue;

                if (!cls.Props.TryGetValue(key, out var prop))
                {
                    // Not in the type-model — an open-world assertion. Preserved as a raw token.
                    statements.Add(new Statement(key, new RawScalar(Lexical(raw))));
                    continue;
                }

                if (IsList(raw, out var items))
                {
                    var values = new List<CanonicalValue>();
                    foreach (var item in items) values.Add(Value(prop, item));
                    statements.Add(new Statement(prop.Predicate, new KList(values)));
                }
                else
                {
                    statements.Add(new Statement(prop.Predicate, Value(prop, raw)));
                }
            }

            // Open-world extras outside the type-model, keyed by their own predicate URI.
            if (node.TryGetValue("$extra", out var extraObj) && extraObj is IReadOnlyDictionary<string, object> extra)
            {
                foreach (var kv in extra)
                {
                    if (kv.Value == null) continue;
                    statements.Add(new Statement(kv.Key, new RawScalar(Lexical(kv.Value))));
                }
            }

            return statements;
        }

        private static CanonicalValue Value(CodecProp prop, object raw)
        {
            if (prop.Kind == "object")
            {
                if (raw is IReadOnlyDictionary<string, object> map && map.TryGetValue("$ref", out var refUri))
                    return new Reference(Convert.ToString(refUri, CultureInfo.InvariantCulture));
                throw new ArgumentException(
                    "Embedded object values are not yet supported by the codec runtime; " +
                    "pass a reference ({\"$ref\": ...}).");
            }

            Carrier? carrier = CarrierMap.CarrierOf(EntityUri.Parse(prop.Datatype));
            if (carrier == null) return new RawScalar(Lexical(raw));
            return new TypedScalar(carrier.Value, Lexical(raw));
        }

        /// <summary>The raw lexical token of a scalar — the input the canonical form normalizes.</summary>
        private static string Lexical(object value)
        {
            switch (value)
            {
                case bool b: return b ? "true" : "false";
                case string s: return s;
                case byte n: return n.ToString(CultureInfo.InvariantCulture);
                case sbyte n: return n.ToString(CultureInfo.InvariantCulture);
                case short n: return n.ToString(CultureInfo.InvariantCulture);
                case ushort n: return n.ToString(CultureInfo.InvariantCulture);
                case int n: return n.ToString(CultureInfo.InvariantCulture);
                case uint n: return n.ToString(CultureInfo.InvariantCulture);
                case long n: return n.ToString(CultureInfo.InvariantCulture);
                case ulong n: return n.ToString(CultureInfo.InvariantCulture);
                case float f: return f.ToString("R", CultureInfo.InvariantCulture);
                case double d: return d.ToString("R", CultureInfo.InvariantCulture);
                case decimal m: return m.ToString(CultureInfo.InvariantCulture);
                default: return Convert.ToString(value, CultureInfo.InvariantCulture);
            }
        }

        // -- Wire (de)serialization ---------------------------------------------

        /// <summary>
        /// Serialize a typed node to its normalized-JSON wire form. The modeled fields
        /// (drop null) come first in node order; then <c>$extra</c> entries spread as
        /// sibling fields AFTER — a modeled field wins a name collision, and no
        /// <c>$extra</c> key rides on the wire (<c>[JsonExtensionData]</c> semantics).
        /// </summary>
        public static Dictionary<string, object> Serialize(IReadOnlyDictionary<string, object> node)
        {
            var outMap = new Dictionary<string, object>();
            foreach (var kv in node)
            {
                if (kv.Key == "$extra" || kv.Value == null) continue;
                outMap[kv.Key] = kv.Value;
            }
            if (node.TryGetValue("$extra", out var extraObj) && extraObj is IReadOnlyDictionary<string, object> extra)
            {
                foreach (var kv in extra)
                    if (kv.Value != null && !outMap.ContainsKey(kv.Key))
                        outMap[kv.Key] = kv.Value;
            }
            return outMap;
        }

        /// <summary>
        /// Parse normalized JSON into a typed node. <c>$</c>-envelope keys and the fields
        /// modeled on the node's <c>$type</c> stay top-level; every other key is an
        /// open-world assertion collected into <c>$extra</c> so a strongly-typed consumer
        /// round-trips it losslessly. Requires a string <c>$type</c> in the schema.
        /// </summary>
        public static Dictionary<string, object> Deserialize(IReadOnlyDictionary<string, object> json, CodecSchema schema)
        {
            if (!json.TryGetValue("$type", out var typeObj) || !(typeObj is string typeUri))
                throw new ArgumentException("Cannot deserialize: missing string $type");
            if (!schema.Classes.TryGetValue(typeUri, out var cls))
                throw new ArgumentException("Cannot deserialize: no schema for type " + typeUri);

            var node = new Dictionary<string, object> { ["$type"] = typeUri };
            Dictionary<string, object> extra = null;
            foreach (var kv in json)
            {
                if (kv.Key == "$type") continue;
                if (kv.Key.StartsWith("$", StringComparison.Ordinal) || cls.Props.ContainsKey(kv.Key))
                    node[kv.Key] = kv.Value;
                else
                    (extra ?? (extra = new Dictionary<string, object>()))[kv.Key] = kv.Value;
            }
            if (extra != null) node["$extra"] = extra;
            return node;
        }

        // -- Helpers -------------------------------------------------------------

        private static string GetString(IReadOnlyDictionary<string, object> node, string key)
            => node.TryGetValue(key, out var v) && v is string s ? s : null;

        private static bool IsList(object raw, out IEnumerable items)
        {
            // A reference map is IEnumerable<KeyValuePair> but must NOT be treated as a list.
            if (raw is string || raw is IDictionary)
            {
                items = null;
                return false;
            }
            if (raw is IEnumerable e)
            {
                items = e;
                return true;
            }
            items = null;
            return false;
        }
    }
}
