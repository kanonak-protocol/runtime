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
    /// numeric), an <see cref="IReadOnlyList{T}"/> of those, a reference map
    /// (<c>{ "$ref": uri }</c>), or an embedded node (a map without <c>$ref</c>/<c>$id</c>).
    /// <c>$extra</c> is a map keyed by predicate URI.
    /// </summary>
    public static class Codec
    {
        /// <summary>Reserved <c>$</c>-envelope keys — never emitted as ontology statements.
        /// <c>$name</c> (0.2.0) carries an embedded value's authored dict-key — hash-relevant.
        /// <c>$types</c> (0.4.0, runtime#10) carries a multi-typed node's FULL type set.</summary>
        private static readonly HashSet<string> EnvelopeKeys = new HashSet<string>
        {
            "$type", "$types", "$id", "$name", "$contentHash", "$version", "$extra",
        };

        /// <summary>Lexicographic comparison by UTF-8 byte sequence (== code-point order).</summary>
        private static int CompareUtf8(string a, string b)
        {
            byte[] ab = System.Text.Encoding.UTF8.GetBytes(a);
            byte[] bb = System.Text.Encoding.UTF8.GetBytes(b);
            int n = Math.Min(ab.Length, bb.Length);
            for (int i = 0; i < n; i++)
            {
                int d = ab[i] - bb[i];
                if (d != 0) return d;
            }
            return ab.Length - bb.Length;
        }

        /// <summary>
        /// Validate a node-or-embedded's <c>$types</c> envelope (0.4.0, runtime#10)
        /// and return the validated set, or null when the node is single-typed.
        /// Invariants: sorted by UTF-8 bytes, at least two members, no duplicates,
        /// and <c>$type</c> (the dispatch key, chosen by the schema layer's primary
        /// rule) is a member. Enforced wherever the envelope is touched — Serialize,
        /// Deserialize, and canonicalization — so a producer fails at emit time and
        /// a reader never masks a nondeterministic emitter by silently repairing
        /// the set.
        /// </summary>
        private static List<string> ValidatedTypes(IReadOnlyDictionary<string, object> map, string where)
        {
            if (!map.TryGetValue("$types", out var raw) || raw == null) return null;
            if (raw is string || raw is IDictionary || !(raw is IEnumerable rawItems))
                throw new ArgumentException(where + ": $types must be a list of non-empty type URIs");

            var types = new List<string>();
            foreach (var item in rawItems)
            {
                if (!(item is string s) || s.Length == 0)
                    throw new ArgumentException(where + ": $types must be a list of non-empty type URIs");
                types.Add(s);
            }
            if (types.Count < 2)
                throw new ArgumentException(
                    where + ": $types with " + types.Count + " member(s) is forbidden — a single-typed " +
                    "node carries only $type (a second encoding of the same content would be hash-ambiguous)");
            for (int i = 1; i < types.Count; i++)
            {
                int cmp = CompareUtf8(types[i - 1], types[i]);
                if (cmp == 0)
                    throw new ArgumentException(where + ": $types carries duplicate member " + types[i]);
                if (cmp > 0)
                    throw new ArgumentException(
                        where + ": $types is not sorted by UTF-8 bytes (" + types[i - 1] +
                        " sorts after " + types[i] + ") — ordering is the producer's job, never the reader's");
            }
            string primary = GetString(map, "$type");
            if (primary == null || !types.Contains(primary))
                throw new ArgumentException(
                    where + ": $type (" + (primary ?? "null") + ") must be present and a member of $types");
            return types;
        }

        /// <summary>
        /// Recursively validate every <c>$types</c> envelope in a wire value (the
        /// node itself and any embedded node at any depth). Shared by
        /// <see cref="Serialize"/> (the producer fails at emit time) and
        /// <see cref="Deserialize"/> (the strict reader rejects rather than repairs).
        /// </summary>
        private static void AssertTypesEnvelopes(object value, string where)
        {
            if (value is string) return;
            if (value is IReadOnlyDictionary<string, object> map)
            {
                if (map.ContainsKey("$types")) ValidatedTypes(map, where);
                foreach (var kv in map)
                {
                    if (kv.Key != "$types") AssertTypesEnvelopes(kv.Value, where + "." + kv.Key);
                }
                return;
            }
            if (value is IDictionary) return;
            if (value is IEnumerable items)
            {
                int i = 0;
                foreach (var item in items) AssertTypesEnvelopes(item, where + "[" + i++ + "]");
            }
        }

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
            string id = GetString(node, "$id");
            var types = ValidatedTypes(node, "Node " + (string.IsNullOrEmpty(id) ? "(no $id)" : id));
            string typeUri = GetString(node, "$type");
            if (string.IsNullOrEmpty(typeUri)) throw new ArgumentException("node is missing $type");
            if (!schema.Classes.TryGetValue(typeUri, out var cls))
                throw new ArgumentException("no schema for type " + typeUri);

            // The rdf:type triple(s) every resource carries: one per $types member
            // for a multi-typed node (in $types' UTF-8 sorted order), else $type.
            var statements = new List<Statement>();
            foreach (var member in types ?? new List<string> { typeUri })
            {
                statements.Add(new Statement(schema.TypePredicate, new Reference(member)));
            }
            statements.AddRange(FieldStatements(node, cls, schema));
            return statements;
        }

        /// <summary>
        /// The statements for one node-or-embedded's modeled fields + its <c>$extra</c> —
        /// everything except the type triple (subjects always carry one; embeddeds only
        /// when explicitly typed).
        /// </summary>
        private static List<Statement> FieldStatements(
            IReadOnlyDictionary<string, object> source, CodecClass cls, CodecSchema schema)
        {
            var statements = new List<Statement>();

            foreach (var kv in source)
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
                    foreach (var item in items) values.Add(Value(prop, item, schema));
                    // An empty list contributes NO statement — absent and empty are identical
                    // at the canonical layer (the wire Serialize still preserves the empty list).
                    if (values.Count == 0) continue;
                    statements.Add(new Statement(prop.Predicate, new KList(values)));
                }
                else
                {
                    statements.Add(new Statement(prop.Predicate, Value(prop, raw, schema)));
                }
            }

            // Open-world extras outside the type-model, keyed by their own predicate URI.
            if (source.TryGetValue("$extra", out var extraObj) && extraObj is IReadOnlyDictionary<string, object> extra)
            {
                foreach (var kv in extra)
                {
                    if (kv.Value == null) continue;
                    statements.Add(new Statement(kv.Key, new RawScalar(Lexical(kv.Value))));
                }
            }

            return statements;
        }

        private static CanonicalValue Value(CodecProp prop, object raw, CodecSchema schema)
        {
            if (prop.Kind == "object")
            {
                // A node: a reference ({ "$ref": … }) or an embedded resource.
                if (raw is IReadOnlyDictionary<string, object> map)
                {
                    if (map.TryGetValue("$ref", out var refUri))
                        return new Reference(Convert.ToString(refUri, CultureInfo.InvariantCulture));
                    return EmbeddedValue(prop, map, schema);
                }
                throw new ArgumentException(
                    "Object property " + prop.Predicate + " expects a reference " +
                    "({\"$ref\": ...}) or an embedded node (a map), got " +
                    (raw == null ? "null" : raw.GetType().Name));
            }

            Carrier? carrier = CarrierMap.CarrierOf(EntityUri.Parse(prop.Datatype));
            if (carrier == null) return new RawScalar(Lexical(raw));
            return new TypedScalar(carrier.Value, Lexical(raw));
        }

        /// <summary>
        /// Canonicalize an embedded value (0.2.0): a map with no <c>$id</c>, an optional
        /// <c>$name</c> (the authored dict-key — hash-relevant), an optional <c>$type</c>,
        /// and schema-mapped fields. An explicit <c>$type</c> emits a type statement inside
        /// the embedded (hash-relevant even when it equals the range-derived type); without
        /// it, fields map via the containing property's range and NO type statement is
        /// emitted — range-derived typing is inference only.
        /// </summary>
        private static CanonicalValue EmbeddedValue(
            CodecProp prop, IReadOnlyDictionary<string, object> map, CodecSchema schema)
        {
            if (map.ContainsKey("$id"))
                throw new ArgumentException(
                    "An embedded value under " + prop.Predicate + " must not carry $id — " +
                    "to point at a named resource, pass a reference ({\"$ref\": ...}).");

            var types = ValidatedTypes(map, "Embedded value under " + prop.Predicate);
            string explicitType = GetString(map, "$type");
            string clsUri = explicitType ?? prop.Range;
            if (clsUri == null)
                throw new ArgumentException(
                    "Cannot map embedded value under " + prop.Predicate + ": it carries " +
                    "no $type and the property declares no range.");
            if (!schema.Classes.TryGetValue(clsUri, out var cls))
                throw new ArgumentException("no schema for embedded type " + clsUri);

            var statements = FieldStatements(map, cls, schema);
            if (types != null)
            {
                // A multi-typed embedded ($types implies an explicit $type): one type
                // statement per member, in $types (UTF-8 sorted) order — all hash-relevant.
                foreach (var member in types)
                    statements.Add(new Statement(schema.TypePredicate, new Reference(member)));
            }
            else if (explicitType != null)
            {
                statements.Add(new Statement(schema.TypePredicate, new Reference(explicitType)));
            }

            string name = GetString(map, "$name");
            if (string.IsNullOrEmpty(name)) name = null;
            return new Embedded(name, statements);
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
            // Producer-side $types validation, at every depth — fail closest to the bug.
            string where = GetString(node, "$id") ?? GetString(node, "$type") ?? "(node)";
            AssertTypesEnvelopes(node, "serialize " + where);
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
            // Reader-side $types validation, at every depth: an unsorted / singleton /
            // duplicate / non-member set is REJECTED, never silently repaired —
            // determinism belongs to the producer, and a lenient reader would mask a
            // nondeterministic emitter.
            AssertTypesEnvelopes(json, "deserialize " + (GetString(json, "$id") ?? typeUri));
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
