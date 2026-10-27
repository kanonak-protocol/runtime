package org.kanonak.codec;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

import org.kanonak.canonical.CanonicalForm;
import org.kanonak.canonical.CanonicalForm.Package;
import org.kanonak.canonical.CanonicalForm.Statement;
import org.kanonak.canonical.CanonicalForm.Subject;
import org.kanonak.canonical.CanonicalForm.Value;
import org.kanonak.canonical.Carrier;
import org.kanonak.canonical.Coordinate;
import org.kanonak.canonical.Datatypes;

import org.kanonak.codec.CodecSchema.CodecClass;
import org.kanonak.codec.CodecSchema.CodecEnum;
import org.kanonak.codec.CodecSchema.CodecEnumMember;
import org.kanonak.codec.CodecSchema.CodecProp;

/**
 * {@code kanonak-codec} — the generic, ontology-INDEPENDENT codec runtime.
 *
 * <p>Given a {@link CodecSchema} (the per-package metadata a generated SDK embeds)
 * and a set of typed nodes, it builds the language-neutral canonical input model
 * and content-addresses it via {@link CanonicalForm} (the same content-form the
 * Python/TypeScript references and the {@code kanonak hash} CLI produce). It also
 * (de)serializes the normalized-JSON wire form.
 *
 * <p>A node is a plain {@code Map<String, Object>}: the {@code $}-envelope
 * ({@code $type}, {@code $id}, optional {@code $extra}) plus alias-collapsed
 * local-name fields (String / Boolean / Number / List / {@code {"$ref": uri}} as
 * a Map). Self-contained: carriers come from the schema's datatype URIs, and the
 * resolved foundation URIs are embedded by the generator, so hashing needs no
 * runtime ontology resolution.
 */
public final class Codec {
    private Codec() {}

    /**
     * Reserved {@code $}-envelope keys — never emitted as ontology statements.
     * {@code $name} (0.2.0) carries an embedded value's authored dict-key — hash-relevant.
     * {@code $types} (0.4.0, runtime#10) carries a multi-typed node's FULL type set.
     */
    private static final Set<String> ENVELOPE_KEYS =
        Set.of("$type", "$types", "$id", "$name", "$contentHash", "$version", "$extra");

    /** Lexicographic comparison by UTF-8 byte sequence (== code-point order). */
    private static int compareUtf8(String a, String b) {
        return java.util.Arrays.compareUnsigned(
            a.getBytes(java.nio.charset.StandardCharsets.UTF_8),
            b.getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }

    /**
     * Validate a node-or-embedded's {@code $types} envelope (0.4.0, runtime#10)
     * and return the validated set, or {@code null} when the node is
     * single-typed. Invariants: sorted by UTF-8 bytes, at least two members, no
     * duplicates, and {@code $type} (the dispatch key, chosen by the schema
     * layer's primary rule) is a member. Enforced wherever the envelope is
     * touched — serialize, deserialize, and canonicalization — so a producer
     * fails at emit time and a reader never masks a nondeterministic emitter by
     * silently repairing the set.
     */
    private static List<String> validatedTypes(Map<String, Object> map, String where) {
        Object raw = map.get("$types");
        if (raw == null) {
            return null;
        }
        if (!(raw instanceof List<?> list)) {
            throw new IllegalArgumentException(where + ": $types must be a list of non-empty type URIs");
        }
        List<String> types = new ArrayList<>(list.size());
        for (Object item : list) {
            if (!(item instanceof String s) || s.isEmpty()) {
                throw new IllegalArgumentException(where + ": $types must be a list of non-empty type URIs");
            }
            types.add(s);
        }
        if (types.size() < 2) {
            throw new IllegalArgumentException(
                where + ": $types with " + types.size() + " member(s) is forbidden — a single-typed "
                    + "node carries only $type (a second encoding of the same content would be hash-ambiguous)");
        }
        for (int i = 1; i < types.size(); i++) {
            int cmp = compareUtf8(types.get(i - 1), types.get(i));
            if (cmp == 0) {
                throw new IllegalArgumentException(
                    where + ": $types carries duplicate member " + types.get(i));
            }
            if (cmp > 0) {
                throw new IllegalArgumentException(
                    where + ": $types is not sorted by UTF-8 bytes (" + types.get(i - 1)
                        + " sorts after " + types.get(i)
                        + ") — ordering is the producer's job, never the reader's");
            }
        }
        Object primary = map.get("$type");
        if (!(primary instanceof String p) || !types.contains(p)) {
            throw new IllegalArgumentException(
                where + ": $type (" + primary + ") must be present and a member of $types");
        }
        return types;
    }

    /**
     * Recursively validate every {@code $types} envelope in a wire value (the
     * node itself and any embedded node at any depth). Shared by
     * {@link #serialize} (the producer fails at emit time) and
     * {@link #deserialize} (the strict reader rejects rather than repairs).
     */
    @SuppressWarnings("unchecked")
    private static void assertTypesEnvelopes(Object value, String where) {
        if (value instanceof List<?> list) {
            for (int i = 0; i < list.size(); i++) {
                assertTypesEnvelopes(list.get(i), where + "[" + i + "]");
            }
            return;
        }
        if (value instanceof Map<?, ?> m) {
            Map<String, Object> map = (Map<String, Object>) m;
            if (map.containsKey("$types")) {
                validatedTypes(map, where);
            }
            for (Map.Entry<String, Object> e : map.entrySet()) {
                if (!"$types".equals(e.getKey())) {
                    assertTypesEnvelopes(e.getValue(), where + "." + e.getKey());
                }
            }
        }
    }

    /**
     * Every reference value ({@code {"$ref": uri}}) in a wire value, at any
     * depth, must address a NAMED resource:
     * {@code publisher/package[@version]/name}, no URI fragment (runtime#6).
     * Fragments address embedded resources for navigation and deep-linking;
     * they are not part of the reference graph, and accepting one here would
     * establish that capability by accident. Shared by {@link #serialize},
     * {@link #deserialize} and canonicalization, so all three reject the same
     * input.
     */
    @SuppressWarnings("unchecked")
    private static void assertReferenceValues(Object value, String where) {
        if (value instanceof List<?> list) {
            for (int i = 0; i < list.size(); i++) {
                assertReferenceValues(list.get(i), where + "[" + i + "]");
            }
            return;
        }
        if (value instanceof Map<?, ?> m) {
            Map<String, Object> map = (Map<String, Object>) m;
            if (map.get("$ref") instanceof String uri) {
                assertNamedReference(uri, where);
            }
            for (Map.Entry<String, Object> e : map.entrySet()) {
                assertReferenceValues(e.getValue(), where + "." + e.getKey());
            }
        }
    }

    private static void assertNamedReference(String uri, String where) {
        if (uri.contains("#")) {
            throw new IllegalArgumentException(
                where + ": reference value '" + uri + "' contains a URI fragment. A reference addresses a named "
                    + "resource (publisher/package[@version]/name); fragments are a navigation convention, not "
                    + "part of the reference graph [fragment-reference]");
        }
    }

    // -- The compatible class lookup (runtime#28) --------------------------------

    /** {@link #classFor}'s outcome: exactly one of {@code cls} (found) or {@code error} (why not). */
    private record ClassMatch(CodecClass cls, String error) {}

    /**
     * THE class lookup (runtime#28). A node is typed with the version of the
     * class its producer's import resolved to; a codec generated from a later
     * COMPATIBLE version of the same package must still read it. So: the exact
     * versioned key first, then the class with the same publisher, package and
     * name whose version can read the written one —
     * {@link Coordinate#isReadableBy} from kanonak-canonical, the protocol's
     * single compatibility rule, pinned in every port. When nothing matches,
     * the error says why, with a bracketed kind the conformance vectors pin:
     * {@code [unknown-type]}, {@code [newer-version]} (the data may use terms
     * this schema lacks), {@code [other-major]}, or {@code [other-minor-line]}
     * (below 1.0.0 the minor is the incompatible line).
     */
    private static ClassMatch classFor(CodecSchema schema, String typeUri) {
        CodecClass exact = schema.classes().get(typeUri);
        if (exact != null) {
            return new ClassMatch(exact, null);
        }
        Coordinate written = coordinateOf(typeUri);
        if (written == null || written.version() == null) {
            return new ClassMatch(null, "no schema for type " + typeUri + " [unknown-type]");
        }
        String key = keyOf(written);

        CodecClass readable = null;
        Coordinate.Version readableVersion = null;
        Coordinate.Version nearest = null;
        for (Map.Entry<String, CodecClass> e : schema.classes().entrySet()) {
            Coordinate c = coordinateOf(e.getKey());
            if (c == null || c.version() == null || !keyOf(c).equals(key)) {
                continue;
            }
            if (Coordinate.isReadableBy(written.version(), c.version())) {
                if (readable == null || compareVersions(c.version(), readableVersion) > 0) {
                    readable = e.getValue();
                    readableVersion = c.version();
                }
            } else if (nearest == null || compareVersions(c.version(), nearest) > 0) {
                nearest = c.version();
            }
        }
        if (readable != null) {
            return new ClassMatch(readable, null);
        }
        if (nearest == null) {
            return new ClassMatch(null, "no schema for type " + typeUri + " [unknown-type]");
        }
        return new ClassMatch(null, incompatibleVersion(typeUri, written.version(), nearest));
    }

    private static String incompatibleVersion(String typeUri, Coordinate.Version w, Coordinate.Version r) {
        String ws = formatVersion(w);
        String rs = formatVersion(r);
        if (w.major() != r.major()) {
            return typeUri + " is written at " + ws + ", a different major version than this codec's schema ("
                + rs + ") [other-major]";
        }
        if (w.major() == 0 && w.minor() != r.minor()) {
            return typeUri + " is written at " + ws + "; below 1.0.0 a different minor is a different version "
                + "line than this codec's schema (" + rs + ") [other-minor-line]";
        }
        return typeUri + " is written at " + ws + ", newer than this codec's schema (" + rs
            + "); upgrade the codec to read it [newer-version]";
    }

    /** The parsed coordinate (strict), or {@code null} when the string is not one — never throws. */
    private static Coordinate coordinateOf(String uri) {
        if (uri == null) {
            return null;
        }
        try {
            return Coordinate.parse(uri);
        } catch (IllegalArgumentException e) {
            return null;
        }
    }

    /** The versionless identity {@code publisher/package/name} of a parsed coordinate. */
    private static String keyOf(Coordinate c) {
        return c.publisher() + "/" + c.packageName() + "/" + c.name();
    }

    /** The versionless identity of a URI, or {@code null} when it is not a coordinate. */
    private static String keyOf(String uri) {
        Coordinate c = coordinateOf(uri);
        return c == null ? null : keyOf(c);
    }

    private static int compareVersions(Coordinate.Version a, Coordinate.Version b) {
        if (a.major() != b.major()) {
            return Integer.compare(a.major(), b.major());
        }
        if (a.minor() != b.minor()) {
            return Integer.compare(a.minor(), b.minor());
        }
        return Integer.compare(a.patch(), b.patch());
    }

    private static String formatVersion(Coordinate.Version v) {
        return v.major() + "." + v.minor() + "." + v.patch();
    }

    /**
     * The class to HASH a node or embedded value with: the exact versioned
     * class only. A content hash is computed over the producer's predicate and
     * type URIs, versions included, so a node written at an earlier compatible
     * version cannot be re-hashed with a later schema — its predicates would
     * carry the later version and the hash would differ. When only a
     * compatible class exists, say so ({@code [hash-needs-exact-version]})
     * rather than produce a different hash or the bare "no schema" error.
     */
    private static CodecClass hashClassFor(CodecSchema schema, String typeUri, String what) {
        CodecClass exact = schema.classes().get(typeUri);
        if (exact != null) {
            return exact;
        }
        ClassMatch match = classFor(schema, typeUri);
        if (match.cls() != null) {
            throw new IllegalArgumentException(
                "Cannot hash " + what + " " + typeUri + ": this codec's schema has it at "
                    + match.cls().typeUri() + ", and a content hash is computed with the producer's "
                    + "schema version — hashing needs the exact class [hash-needs-exact-version]");
        }
        throw new IllegalArgumentException("No schema for " + what + " " + typeUri + " [unknown-type]");
    }

    /**
     * Whether a node is an instance of the class {@code classUri} — what a
     * generated type guard asks (runtime#28). Each of the node's types
     * ({@code $types}, else its {@code $type}) is resolved through the same
     * compatible lookup {@link #deserialize} uses, so a node written at an
     * earlier compatible version satisfies the later version's guard; it
     * matches when that class IS {@code classUri} or has it among its
     * {@link CodecClass#ancestors() ancestors}. Classes compare by versionless
     * identity. A type this schema cannot read (unknown, newer, another major)
     * never matches.
     *
     * @throws IllegalArgumentException when {@code classUri} is not a class coordinate
     */
    public static boolean typeMatches(Map<String, Object> node, String classUri, CodecSchema schema) {
        Coordinate target = coordinateOf(classUri);
        if (target == null) {
            throw new IllegalArgumentException("typeMatches: '" + classUri + "' is not a class coordinate");
        }
        String targetKey = keyOf(target);
        List<String> members = new ArrayList<>();
        Object types = node.get("$types");
        if (types instanceof List<?> list) {
            for (Object m : list) {
                if (m instanceof String s) {
                    members.add(s);
                }
            }
        } else if (node.get("$type") instanceof String t && !t.isEmpty()) {
            members.add(t);
        }
        for (String member : members) {
            ClassMatch match = classFor(schema, member);
            if (match.cls() == null) {
                continue;
            }
            if (targetKey.equals(keyOf(match.cls().typeUri()))) {
                return true;
            }
            for (String ancestor : match.cls().ancestors()) {
                if (targetKey.equals(keyOf(ancestor))) {
                    return true;
                }
            }
        }
        return false;
    }

    /**
     * {@link #typeMatches(Map, String, CodecSchema)} over a typed instance —
     * its {@code $types} ({@link KanonakNode#getTypes()}), else its
     * {@code $type} ({@link KanonakNode#getTypeUri()}).
     */
    public static boolean typeMatches(KanonakNode node, String classUri, CodecSchema schema) {
        Map<String, Object> envelope = new LinkedHashMap<>();
        envelope.put("$type", node.getTypeUri());
        envelope.put("$types", node.getTypes());
        return typeMatches(envelope, classUri, schema);
    }

    /**
     * An enumeration member found by {@link #enumMember}: the enumeration
     * class's durable URI (its key in {@code enums}), the member's durable URI
     * as this schema keys it, and the member itself.
     */
    public record EnumMemberMatch(String enumType, String uri, CodecEnumMember member) {}

    /**
     * The enumeration member a {@code {"$ref": ...}} names (runtime#28): the
     * exact versioned key first, then the same member at a version that can
     * read the written one — so a member referenced at an earlier compatible
     * version of the package resolves against a later schema. {@code null}
     * when this schema has no such member, which, as for {@code enums} itself,
     * means "not mine", never "invalid".
     */
    public static EnumMemberMatch enumMember(CodecSchema schema, String ref) {
        if (ref == null || schema.enums() == null) {
            return null;
        }
        for (Map.Entry<String, CodecEnum> e : schema.enums().entrySet()) {
            CodecEnumMember member = e.getValue().members().get(ref);
            if (member != null) {
                return new EnumMemberMatch(e.getKey(), ref, member);
            }
        }
        Coordinate written = coordinateOf(ref);
        if (written == null || written.version() == null) {
            return null;
        }
        String key = keyOf(written);
        EnumMemberMatch best = null;
        Coordinate.Version bestVersion = null;
        for (Map.Entry<String, CodecEnum> e : schema.enums().entrySet()) {
            for (Map.Entry<String, CodecEnumMember> m : e.getValue().members().entrySet()) {
                Coordinate c = coordinateOf(m.getKey());
                if (c == null || c.version() == null || !keyOf(c).equals(key)) {
                    continue;
                }
                if (!Coordinate.isReadableBy(written.version(), c.version())) {
                    continue;
                }
                if (best == null || compareVersions(c.version(), bestVersion) > 0) {
                    best = new EnumMemberMatch(e.getKey(), m.getKey(), m.getValue());
                    bestVersion = c.version();
                }
            }
        }
        return best;
    }

    /**
     * The raw lexical token of a scalar — the input the canonical form normalizes.
     * Boolean → {@code "true"}/{@code "false"}; String → verbatim; Number → a plain
     * decimal string with no locale or trailing-zero/scientific artifacts.
     */
    static String lexical(Object value) {
        if (value instanceof Boolean b) {
            return b ? "true" : "false";
        }
        if (value instanceof String s) {
            return s;
        }
        if (value instanceof JsonNumber n) {
            // A JSON numeric literal whose exact source token was retained.
            return n.token();
        }
        if (value instanceof Long || value instanceof Integer || value instanceof Short
            || value instanceof Byte || value instanceof java.math.BigInteger) {
            return value.toString();
        }
        if (value instanceof java.math.BigDecimal bd) {
            return bd.toPlainString();
        }
        if (value instanceof Double || value instanceof Float) {
            // Render without scientific notation / trailing-zero noise.
            return new java.math.BigDecimal(value.toString()).stripTrailingZeros().toPlainString();
        }
        return String.valueOf(value);
    }

    @SuppressWarnings("unchecked")
    private static Value valueOf(CodecProp prop, Object raw, CodecSchema schema) {
        if ("object".equals(prop.kind())) {
            // A node: a reference ({"$ref": ...}) or an embedded resource.
            if (raw instanceof Map<?, ?> m) {
                Map<String, Object> map = (Map<String, Object>) m;
                if (map.containsKey("$ref")) {
                    return new Value.Ref(String.valueOf(map.get("$ref")));
                }
                return embeddedValue(prop, map, schema);
            }
            throw new IllegalArgumentException(
                "Object property " + prop.predicate() + " expects a reference ({\"$ref\": ...}) "
                    + "or an embedded node (a map), got "
                    + (raw == null ? "null" : raw.getClass().getSimpleName()));
        }
        Carrier carrier = Carrier.of(prop.datatype());
        if (carrier == null) {
            return new Value.Raw(lexical(raw));
        }
        return new Value.Typed(carrier, lexical(raw));
    }

    /**
     * Canonicalize an embedded value (0.2.0): a map with no {@code $id}, an
     * optional {@code $name} (the authored dict-key — hash-relevant), an optional
     * {@code $type}, and schema-mapped fields. An explicit {@code $type} emits a
     * type statement inside the embedded (hash-relevant even when it equals the
     * range-derived type); without it, fields map via the containing property's
     * {@code range} and NO type statement is emitted — range-derived typing is
     * inference only.
     */
    private static Value embeddedValue(CodecProp prop, Map<String, Object> map, CodecSchema schema) {
        if (map.containsKey("$id")) {
            throw new IllegalArgumentException(
                "An embedded value under " + prop.predicate() + " must not carry $id — "
                    + "to point at a named resource, pass a reference ({\"$ref\": ...}).");
        }
        List<String> types = validatedTypes(map, "Embedded value under " + prop.predicate());
        String explicitType = map.get("$type") instanceof String t ? t : null;
        String clsUri = explicitType != null ? explicitType : prop.range();
        if (clsUri == null) {
            throw new IllegalArgumentException(
                "Cannot map embedded value under " + prop.predicate() + ": it carries no $type "
                    + "and the property declares no range.");
        }
        CodecClass cls = hashClassFor(schema, clsUri, "embedded type");

        List<Statement> statements = fieldStatements(map, cls, schema);
        if (types != null) {
            // A multi-typed embedded ($types implies an explicit $type): one type
            // statement per member, in $types (UTF-8 sorted) order — all hash-relevant.
            for (String member : types) {
                statements.add(new Statement(schema.typePredicate(), new Value.Ref(member)));
            }
        } else if (explicitType != null) {
            statements.add(new Statement(schema.typePredicate(), new Value.Ref(explicitType)));
        }
        String name = map.get("$name") instanceof String n && !n.isEmpty() ? n : null;
        return new Value.Embed(name, statements);
    }

    /**
     * The statements for one node-or-embedded's modeled fields plus its
     * {@code $extra} — everything except the type triple (subjects always carry
     * one; embeddeds only when explicitly typed).
     */
    @SuppressWarnings("unchecked")
    private static List<Statement> fieldStatements(Map<String, Object> source, CodecClass cls, CodecSchema schema) {
        List<Statement> statements = new ArrayList<>();

        for (Map.Entry<String, Object> entry : source.entrySet()) {
            String key = entry.getKey();
            Object raw = entry.getValue();
            if (ENVELOPE_KEYS.contains(key) || raw == null) {
                continue;
            }
            CodecProp prop = cls.props().get(key);
            if (prop == null) {
                throw undeclaredStatement(cls, "\"" + key + "\"");
            }
            if (raw instanceof List<?> list) {
                // An empty list contributes NO statement — absent and empty are
                // identical at the canonical layer (the wire serialize still
                // preserves the empty list).
                if (list.isEmpty()) {
                    continue;
                }
                List<Value> items = new ArrayList<>(list.size());
                for (Object item : list) {
                    items.add(valueOf(prop, item, schema));
                }
                statements.add(new Statement(prop.predicate(), new Value.KList(items)));
            } else {
                statements.add(new Statement(prop.predicate(), valueOf(prop, raw, schema)));
            }
        }

        // Open-world extras ride through serialize and deserialize, but this codec
        // can't type them, so it refuses to hash them rather than hash them as text.
        Object extra = source.get("$extra");
        if (extra instanceof Map<?, ?> extraMap) {
            for (Map.Entry<String, Object> e : ((Map<String, Object>) extraMap).entrySet()) {
                if (e.getValue() == null) {
                    continue;
                }
                throw undeclaredStatement(cls, "$extra statement " + e.getKey());
            }
        }
        return statements;
    }

    /**
     * A codec holds one package's schema, so it can't know the range of a
     * predicate another package declares on its classes (open-world augmentation).
     * Hashing such a statement as untyped text would give a hash that differs from
     * {@code kanonak hash}'s without a word, so hashing refuses it, naming it
     * ({@code [hash-undeclared-statement]}). Serializing and deserializing carry it.
     */
    private static IllegalArgumentException undeclaredStatement(CodecClass cls, String what) {
        return new IllegalArgumentException(
            "Cannot hash " + cls.typeUri() + "'s " + what + ": this codec's schema doesn't declare it, "
                + "so it can't type its value. Hash it with the SDK, over the graph [hash-undeclared-statement]");
    }

    private static List<Statement> statementsFor(Map<String, Object> node, CodecSchema schema) {
        Object id = node.get("$id");
        String where = "Node " + (id instanceof String s && !s.isEmpty() ? s : "(no $id)");
        List<String> types = validatedTypes(node, where);
        assertReferenceValues(node, where);
        Object typeUri = node.get("$type");
        if (!(typeUri instanceof String) || ((String) typeUri).isEmpty()) {
            throw new IllegalArgumentException("node is missing $type");
        }
        CodecClass cls = hashClassFor(schema, (String) typeUri, "type");

        List<Statement> statements = new ArrayList<>();
        // The rdf:type triple(s) every resource carries: one per $types member for
        // a multi-typed node (in $types' UTF-8 sorted order), else the single $type.
        for (String member : types != null ? types : List.of((String) typeUri)) {
            statements.add(new Statement(schema.typePredicate(), new Value.Ref(member)));
        }
        statements.addAll(fieldStatements(node, cls, schema));
        return statements;
    }

    /**
     * Build the canonical input model: a subject per node plus the synthesized
     * package-wrapper subject (raw label + {@code Package} type), exactly the
     * subject set {@code kanonak hash} produces for the equivalent authored
     * package. Statement/subject ordering is irrelevant (the canonical form
     * orders by predicate/URI UTF-8 bytes).
     */
    public static Package buildPackage(List<Map<String, Object>> nodes, CodecSchema schema, PackageContext pkg) {
        List<Subject> subjects = new ArrayList<>();
        for (Map<String, Object> node : nodes) {
            Object id = node.get("$id");
            if (!(id instanceof String) || ((String) id).isEmpty()) {
                throw new IllegalArgumentException("node is missing $id");
            }
            subjects.add(new Subject((String) id, statementsFor(node, schema)));
        }

        String pkgUri = pkg.publisher() + "/" + pkg.packageName() + "@" + pkg.version() + "/" + pkg.packageName();
        List<Statement> pkgStatements = new ArrayList<>();
        if (pkg.label() != null) {
            pkgStatements.add(new Statement(schema.labelPredicate(), new Value.Raw(pkg.label())));
        }
        pkgStatements.add(new Statement(schema.typePredicate(), new Value.Ref(schema.packageTypeUri())));
        subjects.add(new Subject(pkgUri, pkgStatements));

        return new Package(subjects);
    }

    /** The canonical form (the {@code {subjects:[...]}} JSON) of a package from nodes. */
    public static String canonicalForm(List<Map<String, Object>> nodes, CodecSchema schema, PackageContext pkg) {
        return CanonicalForm.serialize(buildPackage(nodes, schema, pkg));
    }

    /** The {@code sha256:} content hash of a package from nodes — matches {@code kanonak hash}. */
    public static String contentHash(List<Map<String, Object>> nodes, CodecSchema schema, PackageContext pkg) {
        return CanonicalForm.hash(buildPackage(nodes, schema, pkg));
    }

    /**
     * Serialize a typed node to its normalized-JSON wire form. {@code $extra}
     * entries ride as sibling fields after the modeled ones; a modeled field wins
     * a name collision ({@code [JsonExtensionData]} semantics). {@code null}
     * values are dropped and no {@code $extra} key appears on the wire.
     *
     * @deprecated Use {@link #serializeExact(Map, CodecSchema)}, which writes
     *     integer and decimal values in their JSON forms (runtime#54).
     */
    @Deprecated
    @SuppressWarnings("unchecked")
    public static Map<String, Object> serialize(Map<String, Object> node) {
        // Producer-side $types validation, at every depth — fail closest to the bug.
        Object where = node.get("$id") instanceof String s && !s.isEmpty() ? s : node.get("$type");
        assertTypesEnvelopes(node, "serialize " + (where != null ? where : "(node)"));
        assertReferenceValues(node, "serialize " + (where != null ? where : "(node)"));
        Map<String, Object> out = new LinkedHashMap<>();
        for (Map.Entry<String, Object> entry : node.entrySet()) {
            if ("$extra".equals(entry.getKey()) || entry.getValue() == null) {
                continue;
            }
            out.put(entry.getKey(), entry.getValue());
        }
        Object extra = node.get("$extra");
        if (extra instanceof Map<?, ?> extraMap) {
            for (Map.Entry<String, Object> e : ((Map<String, Object>) extraMap).entrySet()) {
                if (e.getValue() != null && !out.containsKey(e.getKey())) {
                    out.put(e.getKey(), e.getValue());
                }
            }
        }
        return out;
    }

    /**
     * Parse normalized JSON into a typed node. {@code $}-envelope keys and the
     * fields modeled on the node's {@code $type} stay top-level; every other key
     * is an open-world assertion collected into {@code $extra}. Requires a string
     * {@code $type} (the one field that cannot be inferred) and a class in the
     * schema that can read it: the exact class, or the same class at a
     * compatible later version (runtime#28 — the same lookup {@link #typeMatches}
     * uses). The node keeps the {@code $type} it was WRITTEN with; the
     * producer's bytes are the producer's.
     *
     * @deprecated Use {@link #deserializeExact(Map, CodecSchema)}, which reads
     *     integer and decimal values exactly (runtime#54).
     */
    @Deprecated
    public static Map<String, Object> deserialize(Map<String, Object> json, CodecSchema schema) {
        Object typeUri = json.get("$type");
        if (!(typeUri instanceof String)) {
            throw new IllegalArgumentException("Cannot deserialize: missing string $type");
        }
        // Reader-side $types validation, at every depth: an unsorted / singleton /
        // duplicate / non-member set is REJECTED, never silently repaired —
        // determinism belongs to the producer, and a lenient reader would mask a
        // nondeterministic emitter.
        Object where = json.get("$id") instanceof String s && !s.isEmpty() ? s : typeUri;
        assertTypesEnvelopes(json, "deserialize " + where);
        assertReferenceValues(json, "deserialize " + where);
        ClassMatch match = classFor(schema, (String) typeUri);
        if (match.cls() == null) {
            throw new IllegalArgumentException("Cannot deserialize: " + match.error());
        }
        CodecClass cls = match.cls();

        Map<String, Object> node = new LinkedHashMap<>();
        node.put("$type", typeUri);
        Map<String, Object> extra = new LinkedHashMap<>();
        for (Map.Entry<String, Object> entry : json.entrySet()) {
            String key = entry.getKey();
            if ("$type".equals(key)) {
                continue;
            }
            if (key.startsWith("$") || cls.props().containsKey(key)) {
                node.put(key, entry.getValue());
            } else {
                extra.put(key, entry.getValue());
            }
        }
        if (!extra.isEmpty()) {
            node.put("$extra", extra);
        }
        return node;
    }

    // -- The JSON forms of integer and decimal values (runtime#54) -----------------

    /**
     * How JSON carries a datatype's values (kanonak-protocol 4.11.0's
     * {@code numbers-in-json}): a wide integer as a string, read from a string
     * or a whole number within ±(2^53 − 1); a decimal as a string only; a
     * narrow integer as a number only.
     */
    private enum JsonForm { WIDE, DECIMAL, NARROW }

    /** A datatype's JSON form, and the range of its values ({@code null} where unbounded). */
    private record JsonNumberForm(JsonForm form, BigDecimal min, BigDecimal max) {
        /** Whether every value fits a {@code long}, so this port holds it as a {@link Long} rather than a {@link BigInteger}. */
        boolean fitsLong() {
            return min != null && max != null
                && min.compareTo(BigDecimal.valueOf(Long.MIN_VALUE)) >= 0
                && max.compareTo(BigDecimal.valueOf(Long.MAX_VALUE)) <= 0;
        }
    }

    private static final String XSD = "kanonak.org/core-xsd/";

    private static final BigDecimal MAX_SAFE_INTEGER = BigDecimal.valueOf((1L << 53) - 1);

    /** Every datatype JSON carries in a form of its own, by versionless key. Any other is left as it is. */
    private static final Map<String, JsonNumberForm> JSON_NUMBER_FORMS = Map.ofEntries(
        Map.entry(XSD + "integer", wide(null, null)),
        Map.entry(XSD + "nonNegativeInteger", wide(BigDecimal.ZERO, null)),
        Map.entry(XSD + "positiveInteger", wide(BigDecimal.ONE, null)),
        Map.entry(XSD + "nonPositiveInteger", wide(null, BigDecimal.ZERO)),
        Map.entry(XSD + "negativeInteger", wide(null, BigDecimal.ONE.negate())),
        Map.entry(XSD + "long", wide(BigDecimal.valueOf(Long.MIN_VALUE), BigDecimal.valueOf(Long.MAX_VALUE))),
        Map.entry(XSD + "unsignedLong",
            wide(BigDecimal.ZERO, new BigDecimal(BigInteger.ONE.shiftLeft(64).subtract(BigInteger.ONE)))),
        Map.entry(XSD + "decimal", new JsonNumberForm(JsonForm.DECIMAL, null, null)),
        Map.entry(XSD + "int", narrow(Integer.MIN_VALUE, Integer.MAX_VALUE)),
        Map.entry(XSD + "short", narrow(Short.MIN_VALUE, Short.MAX_VALUE)),
        Map.entry(XSD + "byte", narrow(Byte.MIN_VALUE, Byte.MAX_VALUE)),
        Map.entry(XSD + "unsignedInt", narrow(0, 0xFFFF_FFFFL)),
        Map.entry(XSD + "unsignedShort", narrow(0, 0xFFFF)),
        Map.entry(XSD + "unsignedByte", narrow(0, 0xFF)));

    private static JsonNumberForm wide(BigDecimal min, BigDecimal max) {
        return new JsonNumberForm(JsonForm.WIDE, min, max);
    }

    private static JsonNumberForm narrow(long min, long max) {
        return new JsonNumberForm(JsonForm.NARROW, BigDecimal.valueOf(min), BigDecimal.valueOf(max));
    }

    /** One value's conversion between its exact type here and its JSON form. */
    @FunctionalInterface
    private interface NumberConversion {
        Object apply(CodecProp prop, JsonNumberForm form, Object value);
    }

    private static IllegalArgumentException jsonNumberError(CodecProp prop, String why) {
        return new IllegalArgumentException(prop.predicate() + ": " + why + " [json-number-form]");
    }

    private static String describe(Object value) {
        if (value instanceof String s) {
            return "the string \"" + s + "\"";
        }
        return value == null ? "null" : "the " + value.getClass().getSimpleName() + " " + value;
    }

    private static BigDecimal inRange(CodecProp prop, JsonNumberForm form, BigDecimal n) {
        if ((form.min() != null && n.compareTo(form.min()) < 0) || (form.max() != null && n.compareTo(form.max()) > 0)) {
            throw jsonNumberError(prop, n + " is outside " + prop.datatype() + "'s range");
        }
        return n;
    }

    private static BigDecimal integerOfLexical(CodecProp prop, String lexical) {
        try {
            return new BigDecimal(Datatypes.canonicalInteger(lexical));
        } catch (IllegalArgumentException notAnInteger) {
            throw jsonNumberError(prop, "\"" + lexical + "\" is not an integer lexical form");
        }
    }

    private static String decimalOfLexical(CodecProp prop, String lexical) {
        try {
            return Datatypes.canonicalDecimal(lexical);
        } catch (IllegalArgumentException notADecimal) {
            throw jsonNumberError(prop, "\"" + lexical + "\" is not a decimal lexical form");
        }
    }

    /** A number's exact value: a {@link JsonNumber}'s token or a {@link Number}'s, else {@code null}. */
    private static BigDecimal numericValue(Object value) {
        try {
            if (value instanceof JsonNumber token) {
                return new BigDecimal(token.token());
            }
            if (value instanceof BigDecimal d) {
                return d;
            }
            if (value instanceof BigInteger i) {
                return new BigDecimal(i);
            }
            if (value instanceof Double || value instanceof Float) {
                return new BigDecimal(((Number) value).doubleValue());
            }
            if (value instanceof Number n) {
                return new BigDecimal(n.toString());
            }
        } catch (NumberFormatException notFinite) {
            return null;
        }
        return null;
    }

    /**
     * A number's value when it is whole and of magnitude at most {@code bound}
     * (any, when {@code null}), else {@code null}. Neither check expands the
     * value, so a token like {@code 1e999999999} costs nothing.
     */
    private static BigDecimal wholeNumber(Object value, BigDecimal bound) {
        BigDecimal n = numericValue(value);
        if (n == null || (n.signum() != 0 && n.stripTrailingZeros().scale() > 0)) {
            return null;
        }
        return bound == null || n.abs().compareTo(bound) <= 0 ? n : null;
    }

    /** A whole value inside its datatype's range, as this port holds it: a {@link Long} where every value fits one, else a {@link BigInteger}. */
    private static Object exactInteger(JsonNumberForm form, BigDecimal n) {
        if (form.fitsLong()) {
            return n.longValueExact();
        }
        return n.toBigIntegerExact();
    }

    /**
     * A value as JSON carries it, to its exact type here: a wide integer to a
     * {@link Long} or a {@link BigInteger}, a decimal to a {@link BigDecimal}
     * of its canonical lexical form, a narrow integer to a {@link Long}.
     */
    private static Object readJsonNumber(CodecProp prop, JsonNumberForm form, Object value) {
        return switch (form.form()) {
            case DECIMAL -> {
                if (!(value instanceof String s)) {
                    throw jsonNumberError(prop,
                        "a decimal is read from a JSON string only, so it can't have been rounded; got " + describe(value));
                }
                yield new BigDecimal(decimalOfLexical(prop, s));
            }
            case WIDE -> {
                BigDecimal n = value instanceof String s ? integerOfLexical(prop, s) : wholeNumber(value, MAX_SAFE_INTEGER);
                if (n == null) {
                    throw jsonNumberError(prop, "a wide integer is read from a string, or from a whole JSON number within "
                        + "±(2^53 − 1); got " + describe(value));
                }
                yield exactInteger(form, inRange(prop, form, n));
            }
            case NARROW -> {
                BigDecimal n = wholeNumber(value, null);
                if (n == null) {
                    throw jsonNumberError(prop, "a narrow integer is read from a whole JSON number only; got " + describe(value));
                }
                yield exactInteger(form, inRange(prop, form, n));
            }
        };
    }

    /**
     * A value of its exact type here, to its JSON form: a wide integer and a
     * decimal to the {@link String} of their canonical lexical form, a narrow
     * integer to a {@link Long}.
     */
    private static Object writeJsonNumber(CodecProp prop, JsonNumberForm form, Object value) {
        return switch (form.form()) {
            case DECIMAL -> {
                if (value instanceof BigDecimal d) {
                    yield decimalOfLexical(prop, d.toPlainString());
                }
                if (value instanceof String s) {
                    yield decimalOfLexical(prop, s);
                }
                throw jsonNumberError(prop,
                    "pass a decimal as a BigDecimal or a string of its digits, which hold it exactly; got " + describe(value));
            }
            case WIDE -> {
                BigDecimal n = value instanceof String s ? integerOfLexical(prop, s)
                    : value instanceof Double ? wholeNumber(value, MAX_SAFE_INTEGER)
                    : value instanceof BigInteger || value instanceof Long || value instanceof Integer
                        || value instanceof Short || value instanceof Byte ? numericValue(value)
                    : null;
                if (n == null) {
                    throw jsonNumberError(prop, "pass a wide integer as a BigInteger, Long, Integer, Short, Byte, string, "
                        + "or whole Double within ±(2^53 − 1); got " + describe(value));
                }
                yield inRange(prop, form, n).toBigIntegerExact().toString();
            }
            case NARROW -> {
                BigDecimal n = value instanceof Number ? wholeNumber(value, null) : null;
                if (n == null) {
                    throw jsonNumberError(prop, "pass a narrow integer as a whole Number; got " + describe(value));
                }
                yield exactInteger(form, inRange(prop, form, n));
            }
        };
    }

    private static JsonNumberForm jsonNumberFormOf(CodecProp prop) {
        if (prop.datatype() == null) {
            return null;
        }
        String key = Coordinate.lenientVersionlessKey(prop.datatype());
        return key == null ? null : JSON_NUMBER_FORMS.get(key);
    }

    private static CodecClass classOrThrow(CodecSchema schema, String typeUri, String what) {
        ClassMatch match = classFor(schema, typeUri);
        if (match.cls() == null) {
            throw new IllegalArgumentException("Cannot convert " + what + ": " + match.error());
        }
        return match.cls();
    }

    /** Each modeled field's value, through lists and embedded values, with its integers and decimals converted. */
    private static Map<String, Object> convertFields(
            Map<String, Object> fields, CodecClass cls, CodecSchema schema, NumberConversion convert) {
        Map<String, Object> out = new LinkedHashMap<>(fields);
        for (Map.Entry<String, CodecProp> e : cls.props().entrySet()) {
            Object value = out.get(e.getKey());
            if (value != null) {
                out.put(e.getKey(), convertValue(e.getValue(), value, schema, convert));
            }
        }
        return out;
    }

    @SuppressWarnings("unchecked")
    private static Object convertValue(CodecProp prop, Object value, CodecSchema schema, NumberConversion convert) {
        if (value instanceof List<?> list) {
            List<Object> out = new ArrayList<>(list.size());
            for (Object item : list) {
                out.add(convertValue(prop, item, schema, convert));
            }
            return out;
        }
        if ("datatype".equals(prop.kind())) {
            JsonNumberForm form = jsonNumberFormOf(prop);
            return form == null ? value : convert.apply(prop, form, value);
        }
        if (value instanceof Map<?, ?> m && !m.containsKey("$ref")) {
            Map<String, Object> map = (Map<String, Object>) m;
            String clsUri = map.get("$type") instanceof String t ? t : prop.range();
            if (clsUri == null) {
                throw new IllegalArgumentException("Cannot convert the embedded value under " + prop.predicate()
                    + ": it carries no $type and the property declares no range.");
            }
            CodecClass cls = classOrThrow(schema, clsUri, "the embedded value under " + prop.predicate());
            return convertFields(map, cls, schema, convert);
        }
        return value;
    }

    /**
     * Serialize a node to normalized JSON, as {@link #serialize} does, writing
     * each integer and decimal value in its JSON form (kanonak-protocol
     * 4.11.0's {@code numbers-in-json}, runtime#54). A wide integer, given as
     * a {@link BigInteger}, {@link Long}, {@link Integer}, {@link Short},
     * {@link Byte}, lexical {@link String} or whole {@link Double} within
     * ±(2^53 − 1), and a decimal, given as a {@link BigDecimal} or lexical
     * {@link String}, are written as the string of their canonical lexical
     * form; a narrow integer, given as a whole {@link Number}, as a
     * {@link Long}. Any other value, or one outside its datatype's range, is
     * refused with a message ending {@code [json-number-form]}.
     */
    public static Map<String, Object> serializeExact(Map<String, Object> node, CodecSchema schema) {
        if (!(node.get("$type") instanceof String typeUri)) {
            throw new IllegalArgumentException("Cannot serialize: missing string $type");
        }
        String what = "node " + (node.get("$id") instanceof String id ? id : typeUri);
        return serialize(convertFields(node, classOrThrow(schema, typeUri, what), schema, Codec::writeJsonNumber));
    }

    /**
     * Parse normalized JSON into a node, as {@link #deserialize} does, reading
     * each integer and decimal value exactly (kanonak-protocol 4.11.0's
     * {@code numbers-in-json}, runtime#54): a wide integer from a string or a
     * whole number within ±(2^53 − 1), to a {@link Long} for {@code xsd:long}
     * and a {@link BigInteger} for the others; a decimal from a string only,
     * to a {@link BigDecimal}; a narrow integer from a whole number only, to a
     * {@link Long}. A number JSON may already have rounded, or a value outside
     * its datatype's range, is refused with a message ending
     * {@code [json-number-form]}.
     */
    public static Map<String, Object> deserializeExact(Map<String, Object> json, CodecSchema schema) {
        Map<String, Object> node = deserialize(json, schema);
        String typeUri = (String) node.get("$type");
        String what = "node " + (node.get("$id") instanceof String id ? id : typeUri);
        return convertFields(node, classOrThrow(schema, typeUri, what), schema, Codec::readJsonNumber);
    }
}
