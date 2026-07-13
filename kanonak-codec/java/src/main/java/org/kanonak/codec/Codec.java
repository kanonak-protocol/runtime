package org.kanonak.codec;

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

import org.kanonak.codec.CodecSchema.CodecClass;
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
        CodecClass cls = schema.classes().get(clsUri);
        if (cls == null) {
            throw new IllegalArgumentException("no schema for embedded type " + clsUri);
        }

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
                // Not in the type-model — an open-world assertion. Preserved as a raw token.
                statements.add(new Statement(key, new Value.Raw(lexical(raw))));
                continue;
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

        Object extra = source.get("$extra");
        if (extra instanceof Map<?, ?> extraMap) {
            for (Map.Entry<String, Object> e : ((Map<String, Object>) extraMap).entrySet()) {
                if (e.getValue() == null) {
                    continue;
                }
                statements.add(new Statement(e.getKey(), new Value.Raw(lexical(e.getValue()))));
            }
        }
        return statements;
    }

    private static List<Statement> statementsFor(Map<String, Object> node, CodecSchema schema) {
        Object id = node.get("$id");
        List<String> types = validatedTypes(node,
            "Node " + (id instanceof String s && !s.isEmpty() ? s : "(no $id)"));
        Object typeUri = node.get("$type");
        if (!(typeUri instanceof String) || ((String) typeUri).isEmpty()) {
            throw new IllegalArgumentException("node is missing $type");
        }
        CodecClass cls = schema.classes().get(typeUri);
        if (cls == null) {
            throw new IllegalArgumentException("no schema for type " + typeUri);
        }

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
     */
    @SuppressWarnings("unchecked")
    public static Map<String, Object> serialize(Map<String, Object> node) {
        // Producer-side $types validation, at every depth — fail closest to the bug.
        Object where = node.get("$id") instanceof String s && !s.isEmpty() ? s : node.get("$type");
        assertTypesEnvelopes(node, "serialize " + (where != null ? where : "(node)"));
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
     * {@code $type} whose class is present in the schema.
     */
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
        CodecClass cls = schema.classes().get(typeUri);
        if (cls == null) {
            throw new IllegalArgumentException("Cannot deserialize: no schema for type " + typeUri);
        }

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
}
