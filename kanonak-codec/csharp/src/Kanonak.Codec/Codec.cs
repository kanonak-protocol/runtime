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

        // -- The compatible class lookup (runtime#28) ---------------------------

        /// <summary>
        /// THE class lookup (runtime#28). A node is typed with the version of the
        /// class its producer's import resolved to; a codec generated from a later
        /// COMPATIBLE version of the same package must still read it. So: the exact
        /// versioned key first, then the class with the same publisher, package and
        /// name whose version can read the written one —
        /// <see cref="Coordinate.IsReadableBy"/> from <c>Kanonak.Canonical</c>, the
        /// protocol's single compatibility rule, pinned in every port. When nothing
        /// matches, returns null and <paramref name="error"/> says why, with a
        /// bracketed kind the conformance vectors pin: <c>[unknown-type]</c>,
        /// <c>[newer-version]</c> (the data may use terms this schema lacks),
        /// <c>[other-major]</c>, or <c>[other-minor-line]</c> (below 1.0.0 the minor
        /// is the incompatible line).
        /// </summary>
        private static CodecClass ClassFor(CodecSchema schema, string typeUri, out string error)
        {
            error = null;
            if (typeUri != null && schema.Classes.TryGetValue(typeUri, out var exact)) return exact;
            Coordinate? written = CoordinateOf(typeUri);
            if (written?.Version == null)
            {
                error = "no schema for type " + typeUri + " [unknown-type]";
                return null;
            }
            CoordinateVersion w = written.Value.Version.Value;
            string key = KeyOf(written.Value);

            CodecClass readable = null;
            CoordinateVersion readableVersion = default;
            CoordinateVersion? nearest = null;
            foreach (var kv in schema.Classes)
            {
                Coordinate? c = CoordinateOf(kv.Key);
                if (c?.Version == null || KeyOf(c.Value) != key) continue;
                CoordinateVersion v = c.Value.Version.Value;
                if (Coordinate.IsReadableBy(w, v))
                {
                    if (readable == null || CompareVersions(v, readableVersion) > 0)
                    {
                        readable = kv.Value;
                        readableVersion = v;
                    }
                }
                else if (nearest == null || CompareVersions(v, nearest.Value) > 0)
                {
                    nearest = v;
                }
            }
            if (readable != null) return readable;
            error = nearest == null
                ? "no schema for type " + typeUri + " [unknown-type]"
                : IncompatibleVersion(typeUri, w, nearest.Value);
            return null;
        }

        private static string IncompatibleVersion(string typeUri, CoordinateVersion w, CoordinateVersion r)
        {
            string ws = FormatVersion(w);
            string rs = FormatVersion(r);
            if (w.Major != r.Major)
                return typeUri + " is written at " + ws + ", a different major version than this codec's schema (" +
                    rs + ") [other-major]";
            if (w.Major == 0 && w.Minor != r.Minor)
                return typeUri + " is written at " + ws + "; below 1.0.0 a different minor is a different version " +
                    "line than this codec's schema (" + rs + ") [other-minor-line]";
            return typeUri + " is written at " + ws + ", newer than this codec's schema (" + rs +
                "); upgrade the codec to read it [newer-version]";
        }

        /// <summary>The parsed coordinate (strict), or null when the string is not one — never throws.</summary>
        private static Coordinate? CoordinateOf(string uri)
        {
            if (uri == null) return null;
            try
            {
                return Coordinate.Parse(uri);
            }
            catch (FormatException)
            {
                return null;
            }
        }

        /// <summary>The versionless identity <c>publisher/package/name</c> of a parsed coordinate.</summary>
        private static string KeyOf(Coordinate c) => c.Publisher + "/" + c.Package + "/" + c.Name;

        /// <summary>The versionless identity of a URI, or null when it is not a coordinate.</summary>
        private static string KeyOf(string uri)
        {
            Coordinate? c = CoordinateOf(uri);
            return c == null ? null : KeyOf(c.Value);
        }

        private static int CompareVersions(CoordinateVersion a, CoordinateVersion b)
        {
            if (a.Major != b.Major) return a.Major.CompareTo(b.Major);
            if (a.Minor != b.Minor) return a.Minor.CompareTo(b.Minor);
            return a.Patch.CompareTo(b.Patch);
        }

        private static string FormatVersion(CoordinateVersion v)
            => v.Major.ToString(CultureInfo.InvariantCulture) + "." +
               v.Minor.ToString(CultureInfo.InvariantCulture) + "." +
               v.Patch.ToString(CultureInfo.InvariantCulture);

        /// <summary>
        /// The class to HASH a node or embedded value with: the exact versioned class
        /// only. A content hash is computed over the producer's predicate and type
        /// URIs, versions included, so a node written at an earlier compatible
        /// version cannot be re-hashed with a later schema — its predicates would
        /// carry the later version and the hash would differ. When only a compatible
        /// class exists, say so (<c>[hash-needs-exact-version]</c>) rather than
        /// produce a different hash or the bare "no schema" error.
        /// </summary>
        private static CodecClass HashClassFor(CodecSchema schema, string typeUri, string what)
        {
            if (schema.Classes.TryGetValue(typeUri, out var exact)) return exact;
            CodecClass compatible = ClassFor(schema, typeUri, out _);
            if (compatible != null)
                throw new ArgumentException(
                    "Cannot hash " + what + " " + typeUri + ": this codec's schema has it at " +
                    compatible.TypeUri + ", and a content hash is computed with the producer's schema " +
                    "version — hashing needs the exact class [hash-needs-exact-version]");
            throw new ArgumentException("No schema for " + what + " " + typeUri + " [unknown-type]");
        }

        /// <summary>
        /// Whether a node is an instance of the class <paramref name="classUri"/> —
        /// what a generated type guard asks (runtime#28). Each of the node's types
        /// (<c>$types</c>, else its <c>$type</c>) is resolved through the same
        /// compatible lookup <see cref="Deserialize"/> uses, so a node written at an
        /// earlier compatible version satisfies the later version's guard; it
        /// matches when that class IS <paramref name="classUri"/> or has it among
        /// its <see cref="CodecClass.Ancestors"/>. Classes compare by versionless
        /// identity. A type this schema cannot read (unknown, newer, another major)
        /// never matches.
        /// </summary>
        /// <exception cref="ArgumentException"><paramref name="classUri"/> is not a class coordinate.</exception>
        public static bool TypeMatches(IReadOnlyDictionary<string, object> node, string classUri, CodecSchema schema)
        {
            Coordinate? target = CoordinateOf(classUri);
            if (target == null)
                throw new ArgumentException("TypeMatches: '" + classUri + "' is not a class coordinate");
            string targetKey = KeyOf(target.Value);

            var members = new List<string>();
            if (node.TryGetValue("$types", out var rawTypes) && rawTypes != null
                && !(rawTypes is string) && !(rawTypes is IDictionary) && rawTypes is IEnumerable items)
            {
                foreach (var item in items)
                    if (item is string s) members.Add(s);
            }
            else
            {
                string single = GetString(node, "$type");
                if (!string.IsNullOrEmpty(single)) members.Add(single);
            }

            foreach (var member in members)
            {
                CodecClass cls = ClassFor(schema, member, out _);
                if (cls == null) continue;
                if (KeyOf(cls.TypeUri) == targetKey) return true;
                if (cls.Ancestors != null)
                {
                    foreach (var ancestor in cls.Ancestors)
                        if (KeyOf(ancestor) == targetKey) return true;
                }
            }
            return false;
        }

        /// <summary>
        /// <see cref="TypeMatches(IReadOnlyDictionary{string, object}, string, CodecSchema)"/>
        /// over a typed instance — its <c>$types</c> (<see cref="KanonakNode.Types"/>),
        /// else its <c>$type</c> (<see cref="KanonakNode.Type"/>).
        /// </summary>
        public static bool TypeMatches(KanonakNode node, string classUri, CodecSchema schema)
            => TypeMatches(
                new Dictionary<string, object> { ["$type"] = node.Type, ["$types"] = node.Types },
                classUri,
                schema);

        /// <summary>
        /// The enumeration member a <c>{"$ref": …}</c> names (runtime#28): the exact
        /// versioned key first, then the same member at a version that can read the
        /// written one — so a member referenced at an earlier compatible version of
        /// the package resolves against a later schema. Null when this schema has no
        /// such member, which, as for <see cref="CodecSchema.Enums"/> itself, means
        /// "not mine", never "invalid".
        /// </summary>
        public static EnumMemberMatch EnumMember(CodecSchema schema, string reference)
        {
            if (reference == null || schema.Enums == null) return null;
            foreach (var e in schema.Enums)
            {
                if (e.Value.Members != null && e.Value.Members.TryGetValue(reference, out var member))
                    return new EnumMemberMatch(e.Key, reference, member);
            }
            Coordinate? written = CoordinateOf(reference);
            if (written?.Version == null) return null;
            CoordinateVersion w = written.Value.Version.Value;
            string key = KeyOf(written.Value);

            EnumMemberMatch best = null;
            CoordinateVersion bestVersion = default;
            foreach (var e in schema.Enums)
            {
                if (e.Value.Members == null) continue;
                foreach (var m in e.Value.Members)
                {
                    Coordinate? c = CoordinateOf(m.Key);
                    if (c?.Version == null || KeyOf(c.Value) != key) continue;
                    CoordinateVersion v = c.Value.Version.Value;
                    if (!Coordinate.IsReadableBy(w, v)) continue;
                    if (best == null || CompareVersions(v, bestVersion) > 0)
                    {
                        best = new EnumMemberMatch(e.Key, m.Key, m.Value);
                        bestVersion = v;
                    }
                }
            }
            return best;
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
            CodecClass cls = HashClassFor(schema, typeUri, "type");

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

            Carrier? carrier = CarrierMap.CarrierOf(prop.Datatype);
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
            CodecClass cls = HashClassFor(schema, clsUri, "embedded type");

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
        /// round-trips it losslessly. Requires a string <c>$type</c> (the one field that
        /// cannot be inferred) and a class in the schema that can read it: the exact
        /// class, or the same class at a compatible later version (runtime#28 — the
        /// same lookup <see cref="TypeMatches(IReadOnlyDictionary{string, object}, string, CodecSchema)"/>
        /// uses). The node keeps the <c>$type</c> it was WRITTEN with; the producer's
        /// bytes are the producer's.
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
            CodecClass cls = ClassFor(schema, typeUri, out var error);
            if (cls == null) throw new ArgumentException("Cannot deserialize: " + error);

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

    /// <summary>An enumeration member found by <see cref="Codec.EnumMember"/>.</summary>
    public sealed class EnumMemberMatch
    {
        /// <summary>The enumeration class's durable URI (its key in <see cref="CodecSchema.Enums"/>).</summary>
        public string EnumType { get; }

        /// <summary>The member's durable URI as this schema keys it.</summary>
        public string Uri { get; }

        /// <summary>The member itself.</summary>
        public CodecEnumMember Member { get; }

        public EnumMemberMatch(string enumType, string uri, CodecEnumMember member)
        {
            EnumType = enumType;
            Uri = uri;
            Member = member;
        }
    }
}
