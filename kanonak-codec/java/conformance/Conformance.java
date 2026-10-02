import java.math.BigDecimal;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import org.kanonak.codec.Codec;
import org.kanonak.codec.CodecSchema;
import org.kanonak.codec.CodecSchema.CodecClass;
import org.kanonak.codec.CodecSchema.CodecProp;
import org.kanonak.codec.JsonNumber;
import org.kanonak.codec.PackageContext;

/**
 * Drives the shared codec vectors through the Java kanonak-codec port and asserts
 * the canonical form, content hash, and normalized-JSON serialize all match the
 * authoritative (TypeScript-generated) expected values. JDK-only: an embedded
 * minimal JSON parser (numbers retained as {@link JsonNumber}) reads the vectors.
 */
public final class Conformance {
    /** Durable VERSIONED member-key formation: publisher/package@version/name. */
    static final java.util.regex.Pattern MEMBER_URI =
        java.util.regex.Pattern.compile("^[^/]+/[^/@]+@\\d+\\.\\d+\\.\\d+/[^/]+$");

    public static void main(String[] args) throws Exception {
        String[] vectorFiles = args.length > 0
            ? args
            : new String[] {"../vectors/codec-vectors.json", "../vectors/codec-vectors-embedded.json"};

        int passed = 0;
        int failed = 0;
        for (String vectors : vectorFiles) {
            int[] counts = runFile(vectors);
            passed += counts[0];
            failed += counts[1];
            System.out.println(vectors + ": " + counts[0] + " passed, " + counts[1] + " failed");
        }

        if (args.length == 0) {
            String typesVectors = "../vectors/codec-vectors-types.json";
            int[] counts = runTypesFile(typesVectors);
            passed += counts[0];
            failed += counts[1];
            System.out.println(typesVectors + ": " + counts[0] + " passed, " + counts[1] + " failed");

            String enumsVectors = "../vectors/codec-vectors-enums.json";
            int[] ec = runEnumsFile(enumsVectors);
            passed += ec[0];
            failed += ec[1];
            System.out.println(enumsVectors + ": " + ec[0] + " passed, " + ec[1] + " failed");

            String compatVectors = "../vectors/codec-vectors-compat.json";
            int[] cc = runCompatFile(compatVectors);
            passed += cc[0];
            failed += cc[1];
            System.out.println(compatVectors + ": " + cc[0] + " passed, " + cc[1] + " failed");
        }

        System.out.println("\n" + passed + " passed, " + failed + " failed");
        System.exit(failed == 0 ? 0 : 1);
    }

    /**
     * The 0.4.0 multi-typed-subjects file (runtime#10). Beyond the standard
     * form/hash/serialize checks it exercises the $types contract: expectError
     * cases must be rejected on ALL THREE surfaces — serialize (the producer
     * fails at emit time), deserialize (the reader rejects, never repairs), and
     * canonicalization — and positive cases must round-trip:
     * deserialize(serialize(x)) preserves $types exactly and re-canonicalizes
     * to the same hash.
     */
    static int[] runTypesFile(String vectors) throws Exception {
        @SuppressWarnings("unchecked")
        Map<String, Object> data = (Map<String, Object>) Json.parse(
            Files.readString(Paths.get(vectors), StandardCharsets.UTF_8));

        CodecSchema schema = parseSchema(asMap(data.get("schema")));

        int passed = 0;
        int failed = 0;

        for (Object co : asList(data.get("cases"))) {
            Map<String, Object> caseObj = asMap(co);
            String cid = (String) caseObj.get("id");
            List<Map<String, Object>> nodes = new ArrayList<>();
            for (Object n : asList(caseObj.get("nodes"))) {
                nodes.add(asMap(n));
            }
            PackageContext pkg = parsePkg(asMap(caseObj.get("pkg")));

            if (Boolean.TRUE.equals(caseObj.get("expectError"))) {
                boolean ok = true;
                if (!rejects(() -> Codec.canonicalForm(nodes, schema, pkg))) {
                    ok = false;
                    System.out.println("FAIL [" + cid + "] expected canonicalize to reject, it did not");
                }
                if (!rejects(() -> nodes.forEach(Codec::serialize))) {
                    ok = false;
                    System.out.println("FAIL [" + cid + "] expected serialize to reject, it did not");
                }
                if (!rejects(() -> nodes.forEach(n -> Codec.deserialize(n, schema)))) {
                    ok = false;
                    System.out.println("FAIL [" + cid + "] expected deserialize to reject, it did not");
                }
                if (ok) {
                    passed++;
                } else {
                    failed++;
                }
                continue;
            }

            String form = Codec.canonicalForm(nodes, schema, pkg);
            String expForm = (String) caseObj.get("expectedCanonicalForm");
            if (form.equals(expForm)) {
                passed++;
            } else {
                failed++;
                System.out.println("FAIL [" + cid + "] canonical form\n  got: " + form + "\n  exp: " + expForm);
            }

            String hash = Codec.contentHash(nodes, schema, pkg);
            String expHash = (String) caseObj.get("expectedHash");
            if (hash.equals(expHash)) {
                passed++;
            } else {
                failed++;
                System.out.println("FAIL [" + cid + "] hash\n  got: " + hash + "\n  exp: " + expHash);
            }

            List<Object> expSerialize = asList(caseObj.get("expectedSerialize"));
            List<Map<String, Object>> roundTripped = new ArrayList<>();
            boolean serOk = true;
            for (int i = 0; i < nodes.size(); i++) {
                Map<String, Object> wire = Codec.serialize(nodes.get(i));
                if (!deepEquals(wire, expSerialize.get(i))) {
                    serOk = false;
                    System.out.println("FAIL [" + cid + "] serialize[" + i + "]\n  got: " + wire
                        + "\n  exp: " + expSerialize.get(i));
                }
                Map<String, Object> back = Codec.deserialize(wire, schema);
                if (!deepEquals(Codec.serialize(back), expSerialize.get(i))) {
                    serOk = false;
                    System.out.println("FAIL [" + cid + "] round-trip serialize[" + i + "] mismatch");
                }
                roundTripped.add(back);
            }
            if (serOk) {
                passed++;
            } else {
                failed++;
            }

            String rtHash = Codec.contentHash(roundTripped, schema, pkg);
            if (rtHash.equals(expHash)) {
                passed++;
            } else {
                failed++;
                System.out.println("FAIL [" + cid + "] round-trip hash\n  got: " + rtHash + "\n  exp: " + expHash);
            }
        }

        return new int[] {passed, failed};
    }

    static boolean rejects(Runnable run) {
        try {
            run.run();
            return false;
        } catch (IllegalArgumentException expected) {
            return true;
        }
    }

    static int[] runFile(String vectors) throws Exception {
        @SuppressWarnings("unchecked")
        Map<String, Object> data = (Map<String, Object>) Json.parse(
            Files.readString(Paths.get(vectors), StandardCharsets.UTF_8));

        CodecSchema schema = parseSchema(asMap(data.get("schema")));

        int passed = 0;
        int failed = 0;

        for (Object co : asList(data.get("cases"))) {
            Map<String, Object> caseObj = asMap(co);
            String cid = (String) caseObj.get("id");
            List<Map<String, Object>> nodes = new ArrayList<>();
            for (Object n : asList(caseObj.get("nodes"))) {
                nodes.add(asMap(n));
            }
            PackageContext pkg = parsePkg(asMap(caseObj.get("pkg")));

            String form = Codec.canonicalForm(nodes, schema, pkg);
            String expForm = (String) caseObj.get("expectedCanonicalForm");
            if (form.equals(expForm)) {
                passed++;
            } else {
                failed++;
                System.out.println("FAIL [" + cid + "] canonical form\n  got: " + form + "\n  exp: " + expForm);
            }

            String hash = Codec.contentHash(nodes, schema, pkg);
            String expHash = (String) caseObj.get("expectedHash");
            if (hash.equals(expHash)) {
                passed++;
            } else {
                failed++;
                System.out.println("FAIL [" + cid + "] hash\n  got: " + hash + "\n  exp: " + expHash);
            }

            List<Object> expSerialize = asList(caseObj.get("expectedSerialize"));
            for (int i = 0; i < nodes.size(); i++) {
                Object got = Codec.serialize(nodes.get(i));
                Object exp = expSerialize.get(i);
                if (deepEquals(got, exp)) {
                    passed++;
                } else {
                    failed++;
                    System.out.println("FAIL [" + cid + "] serialize[" + i + "]\n  got: " + got + "\n  exp: " + exp);
                }
                // deserialize(serialize(node)) recovers the modeled + $extra split.
                Map<String, Object> back = Codec.deserialize(asMap(got), schema);
                if (java.util.Objects.equals(back.get("$type"), nodes.get(i).get("$type"))) {
                    passed++;
                } else {
                    failed++;
                    System.out.println("FAIL [" + cid + "] deserialize[" + i + "] $type");
                }
            }
        }

        return new int[] {passed, failed};
    }

    // -- Schema / package parsing -------------------------------------------------

    static CodecSchema parseSchema(Map<String, Object> s) {
        Map<String, CodecClass> classes = new LinkedHashMap<>();
        for (Map.Entry<String, Object> e : asMap(s.get("classes")).entrySet()) {
            Map<String, Object> c = asMap(e.getValue());
            Map<String, CodecProp> props = new LinkedHashMap<>();
            for (Map.Entry<String, Object> pe : asMap(c.get("props")).entrySet()) {
                Map<String, Object> p = asMap(pe.getValue());
                props.put(pe.getKey(), new CodecProp(
                    (String) p.get("predicate"),
                    (String) p.get("kind"),
                    (String) p.get("datatype"),
                    (String) p.get("range")));
            }
            List<String> ancestors = new ArrayList<>();
            if (c.get("ancestors") != null) {
                for (Object a : asList(c.get("ancestors"))) {
                    ancestors.add((String) a);
                }
            }
            classes.put(e.getKey(), new CodecClass((String) c.get("typeUri"), props, ancestors));
        }
        // Closed sets (0.5.0, runtime#21). NOTE: this mirrors
        // CodecSchema.fromJson by necessity - the harness already holds a
        // parsed map, not the JSON text fromJson wants. The duplication is a
        // known drift hazard (it would have silently produced enum-free
        // schemas here) and is tracked separately.
        Map<String, CodecSchema.CodecEnum> enums = new LinkedHashMap<>();
        Object rawEnums = s.get("enums");
        if (rawEnums != null) {
            for (Map.Entry<String, Object> e : asMap(rawEnums).entrySet()) {
                Map<String, Object> en = asMap(e.getValue());
                Map<String, CodecSchema.CodecEnumMember> members = new LinkedHashMap<>();
                Object rawMembers = en.get("members");
                if (rawMembers != null) {
                    for (Map.Entry<String, Object> me : asMap(rawMembers).entrySet()) {
                        members.put(me.getKey(), new CodecSchema.CodecEnumMember(
                            (String) asMap(me.getValue()).get("label")));
                    }
                }
                enums.put(e.getKey(), new CodecSchema.CodecEnum(
                    (String) en.get("typeUri"), members));
            }
        }
        return new CodecSchema(
            (String) s.get("typePredicate"),
            (String) s.get("labelPredicate"),
            (String) s.get("packageTypeUri"),
            classes,
            enums);
    }

    /**
     * The 0.5.0 enumerations file (`enums`, runtime#21). Beyond the standard
     * form/hash/serialize checks it pins the enumeration contract: the schema
     * parses with `enums` keyed by durable VERSIONED URIs at both levels (a
     * versionless key would look right and miss every lookup); an enumeration
     * STANDS ALONE, with no `classes` twin; `enums` and an enum-ranged `range`
     * are canonicalization-INERT; and expectError cases are rejected at
     * CANONICALIZATION. Unlike the $types file's all-three-surfaces contract
     * this violation is schema-DEPENDENT: serialize is schema-free and
     * deserialize does not recurse into embedded values, so canonicalization is
     * the only surface that can see it.
     */
    static int[] runEnumsFile(String vectors) throws Exception {
        @SuppressWarnings("unchecked")
        Map<String, Object> data = (Map<String, Object>) Json.parse(
            Files.readString(Paths.get(vectors), StandardCharsets.UTF_8));

        CodecSchema schema = parseSchema(asMap(data.get("schema")));

        int passed = 0;
        int failed = 0;

        // -- structural assertions on the schema itself --
        boolean shapeOk = schema.enums() != null && !schema.enums().isEmpty();
        if (!shapeOk) {
            System.out.println("FAIL [enums] schema carries no enums");
        }
        if (schema.enums() != null) {
            for (Map.Entry<String, CodecSchema.CodecEnum> e : schema.enums().entrySet()) {
                String key = e.getKey();
                CodecSchema.CodecEnum en = e.getValue();
                if (!key.equals(en.typeUri())) {
                    System.out.println("FAIL [enums] key " + key + " != typeUri " + en.typeUri());
                    shapeOk = false;
                }
                if (schema.classes().containsKey(key)) {
                    System.out.println("FAIL [enums] " + key + " must NOT also appear in classes");
                    shapeOk = false;
                }
                if (en.members().isEmpty()) {
                    System.out.println("FAIL [enums] " + key + " declares no members");
                    shapeOk = false;
                }
                for (String memberUri : en.members().keySet()) {
                    if (!MEMBER_URI.matcher(memberUri).matches()) {
                        System.out.println("FAIL [enums] member key " + memberUri
                            + " is not a versioned durable URI");
                        shapeOk = false;
                    }
                }
            }
        }
        if (shapeOk) {
            passed++;
        } else {
            failed++;
        }

        // -- cases --
        for (Object co : asList(data.get("cases"))) {
            Map<String, Object> caseObj = asMap(co);
            String cid = (String) caseObj.get("id");
            List<Map<String, Object>> nodes = new ArrayList<>();
            for (Object n : asList(caseObj.get("nodes"))) {
                nodes.add(asMap(n));
            }
            PackageContext pkg = parsePkg(asMap(caseObj.get("pkg")));

            if (Boolean.TRUE.equals(caseObj.get("expectError"))) {
                if (rejects(() -> Codec.canonicalForm(nodes, schema, pkg))) {
                    passed++;
                } else {
                    failed++;
                    System.out.println("FAIL [" + cid
                        + "] expected canonicalization to reject, it did not");
                }
                continue;
            }

            boolean ok = true;
            String form = Codec.canonicalForm(nodes, schema, pkg);
            if (!form.equals(caseObj.get("expectedCanonicalForm"))) {
                ok = false;
                System.out.println("FAIL [" + cid + "] canonical form mismatch");
            }
            String hash = Codec.contentHash(nodes, schema, pkg);
            if (!hash.equals(caseObj.get("expectedHash"))) {
                ok = false;
                System.out.println("FAIL [" + cid + "] hash expected "
                    + caseObj.get("expectedHash") + " got " + hash);
            }
            List<Object> expectedSer = asList(caseObj.get("expectedSerialize"));
            for (int i = 0; i < nodes.size(); i++) {
                if (!deepEquals(Codec.serialize(nodes.get(i)), expectedSer.get(i))) {
                    ok = false;
                    System.out.println("FAIL [" + cid + "] serialize[" + i + "] mismatch");
                }
            }
            if (ok) {
                passed++;
            } else {
                failed++;
            }
        }

        return new int[] {passed, failed};
    }

    /**
     * The 0.6.1 compatibility file (runtime#28): deserialize / typeMatches /
     * enumMember / hashing over a node typed at an earlier compatible version
     * of the schema's package. A rejection must END in the bracketed kind the
     * vector names — the one part of an error message every port reproduces.
     * The schemas go through the production {@link CodecSchema#fromJson} (the
     * parser a generated SDK runs), so {@code ancestors} is read where
     * consumers read it, and a schema without it must still parse.
     */
    static int[] runCompatFile(String vectors) throws Exception {
        @SuppressWarnings("unchecked")
        Map<String, Object> data = (Map<String, Object>) Json.parse(
            Files.readString(Paths.get(vectors), StandardCharsets.UTF_8));

        Map<String, CodecSchema> schemas = new LinkedHashMap<>();
        for (Map.Entry<String, Object> e : asMap(data.get("schemas")).entrySet()) {
            schemas.put(e.getKey(), CodecSchema.fromJson(toJson(e.getValue())));
        }
        PackageContext pkg = parsePkg(asMap(data.get("pkg")));

        int passed = 0;
        int failed = 0;

        for (Object co : asList(data.get("deserializeCases"))) {
            Map<String, Object> c = asMap(co);
            String cid = (String) c.get("id");
            CodecSchema schema = schemas.get((String) c.get("schema"));
            Map<String, Object> input = asMap(c.get("input"));
            boolean ok;
            if (c.get("expectError") != null) {
                ok = rejectsWithKind(() -> Codec.deserialize(input, schema), (String) c.get("expectError"), cid);
            } else {
                Map<String, Object> got = Codec.deserialize(input, schema);
                ok = deepEquals(got, c.get("expected"));
                if (!ok) {
                    System.out.println("FAIL [" + cid + "] deserialize\n  got: " + got + "\n  exp: " + c.get("expected"));
                }
            }
            if (ok) passed++; else failed++;
        }

        for (Object co : asList(data.get("typeMatchesCases"))) {
            Map<String, Object> c = asMap(co);
            String cid = (String) c.get("id");
            boolean got = Codec.typeMatches(asMap(c.get("node")), (String) c.get("classUri"),
                schemas.get((String) c.get("schema")));
            if (Boolean.valueOf(got).equals(c.get("expected"))) {
                passed++;
            } else {
                failed++;
                System.out.println("FAIL [" + cid + "] typeMatches expected " + c.get("expected") + " got " + got);
            }
        }

        for (Object co : asList(data.get("enumMemberCases"))) {
            Map<String, Object> c = asMap(co);
            String cid = (String) c.get("id");
            Codec.EnumMemberMatch m = Codec.enumMember(schemas.get((String) c.get("schema")), (String) c.get("ref"));
            Map<String, Object> got = null;
            if (m != null) {
                got = new LinkedHashMap<>();
                got.put("enumType", m.enumType());
                got.put("uri", m.uri());
                got.put("label", m.member().label());
            }
            if (deepEquals(got, c.get("expected"))) {
                passed++;
            } else {
                failed++;
                System.out.println("FAIL [" + cid + "] enumMember\n  got: " + got + "\n  exp: " + c.get("expected"));
            }
        }

        for (Object co : asList(data.get("hashCases"))) {
            Map<String, Object> c = asMap(co);
            String cid = (String) c.get("id");
            CodecSchema schema = schemas.get((String) c.get("schema"));
            List<Map<String, Object>> nodes = new ArrayList<>();
            for (Object n : asList(c.get("nodes"))) {
                nodes.add(asMap(n));
            }
            boolean ok;
            if (c.get("expectError") != null) {
                ok = rejectsWithKind(() -> Codec.contentHash(nodes, schema, pkg), (String) c.get("expectError"), cid);
            } else {
                String got = Codec.contentHash(nodes, schema, pkg);
                ok = got.equals(c.get("expectedHash"));
                if (!ok) {
                    System.out.println("FAIL [" + cid + "] hash expected " + c.get("expectedHash") + " got " + got);
                }
            }
            if (ok) passed++; else failed++;
        }

        return new int[] {passed, failed};
    }

    /** Whether {@code run} throws with a message ending in {@code [kind]}; reports otherwise. */
    static boolean rejectsWithKind(Runnable run, String kind, String cid) {
        try {
            run.run();
            System.out.println("FAIL [" + cid + "] expected a [" + kind + "] rejection, got a value");
            return false;
        } catch (RuntimeException err) {
            String message = String.valueOf(err.getMessage());
            boolean ok = message.endsWith("[" + kind + "]");
            if (!ok) {
                System.out.println("FAIL [" + cid + "] expected [" + kind + "], got: " + message);
            }
            return ok;
        }
    }

    /**
     * Re-emit a parsed JSON value as JSON text, so a schema the harness already
     * holds as a map can go through the production {@link CodecSchema#fromJson}.
     */
    static String toJson(Object v) {
        if (v == null) {
            return "null";
        }
        if (v instanceof Map<?, ?> m) {
            StringBuilder sb = new StringBuilder("{");
            boolean first = true;
            for (Map.Entry<?, ?> e : m.entrySet()) {
                if (!first) sb.append(',');
                first = false;
                sb.append(toJson(String.valueOf(e.getKey()))).append(':').append(toJson(e.getValue()));
            }
            return sb.append('}').toString();
        }
        if (v instanceof List<?> l) {
            StringBuilder sb = new StringBuilder("[");
            for (int i = 0; i < l.size(); i++) {
                if (i > 0) sb.append(',');
                sb.append(toJson(l.get(i)));
            }
            return sb.append(']').toString();
        }
        if (v instanceof String s) {
            StringBuilder sb = new StringBuilder("\"");
            for (int i = 0; i < s.length(); i++) {
                char ch = s.charAt(i);
                if (ch == '"' || ch == '\\') {
                    sb.append('\\').append(ch);
                } else if (ch < 0x20) {
                    sb.append(String.format("\\u%04x", (int) ch));
                } else {
                    sb.append(ch);
                }
            }
            return sb.append('"').toString();
        }
        if (v instanceof JsonNumber n) {
            return n.token();
        }
        return String.valueOf(v);
    }

    static PackageContext parsePkg(Map<String, Object> p) {
        Object label = p.get("label");
        return new PackageContext(
            (String) p.get("publisher"),
            (String) p.get("packageName"),
            (String) p.get("version"),
            label == null ? null : (String) label);
    }

    // -- Structural deep-equality (NOT key order or JSON text) --------------------

    @SuppressWarnings("unchecked")
    static boolean deepEquals(Object a, Object b) {
        if (a == null || b == null) {
            return a == b;
        }
        if (a instanceof Map<?, ?> && b instanceof Map<?, ?>) {
            Map<String, Object> ma = (Map<String, Object>) a;
            Map<String, Object> mb = (Map<String, Object>) b;
            if (ma.size() != mb.size()) {
                return false;
            }
            for (Map.Entry<String, Object> e : ma.entrySet()) {
                if (!mb.containsKey(e.getKey()) || !deepEquals(e.getValue(), mb.get(e.getKey()))) {
                    return false;
                }
            }
            return true;
        }
        if (a instanceof List<?> la && b instanceof List<?> lb) {
            if (la.size() != lb.size()) {
                return false;
            }
            for (int i = 0; i < la.size(); i++) {
                if (!deepEquals(la.get(i), lb.get(i))) {
                    return false;
                }
            }
            return true;
        }
        if (isNumber(a) && isNumber(b)) {
            return numberValue(a).compareTo(numberValue(b)) == 0;
        }
        return a.equals(b);
    }

    static boolean isNumber(Object o) {
        return o instanceof JsonNumber || o instanceof Number;
    }

    static BigDecimal numberValue(Object o) {
        if (o instanceof JsonNumber n) {
            return new BigDecimal(n.token());
        }
        return new BigDecimal(o.toString());
    }

    @SuppressWarnings("unchecked")
    static Map<String, Object> asMap(Object o) {
        return (Map<String, Object>) o;
    }

    @SuppressWarnings("unchecked")
    static List<Object> asList(Object o) {
        return (List<Object>) o;
    }

    // -- Minimal JSON parser (numbers retained as JsonNumber tokens) --------------

    static final class Json {
        private final String s;
        private int i;

        private Json(String s) {
            this.s = s;
        }

        static Object parse(String s) {
            Json j = new Json(s);
            j.ws();
            Object v = j.value();
            j.ws();
            return v;
        }

        private Object value() {
            char c = s.charAt(i);
            switch (c) {
                case '{': return object();
                case '[': return array();
                case '"': return string();
                case 't': i += 4; return Boolean.TRUE;
                case 'f': i += 5; return Boolean.FALSE;
                case 'n': i += 4; return null;
                default: return number();
            }
        }

        private Map<String, Object> object() {
            Map<String, Object> m = new LinkedHashMap<>();
            i++; ws();
            if (s.charAt(i) == '}') { i++; return m; }
            while (true) {
                ws();
                String key = string();
                ws(); i++; // ':'
                ws();
                m.put(key, value());
                ws();
                char c = s.charAt(i++);
                if (c == '}') return m;
            }
        }

        private List<Object> array() {
            List<Object> a = new ArrayList<>();
            i++; ws();
            if (s.charAt(i) == ']') { i++; return a; }
            while (true) {
                ws();
                a.add(value());
                ws();
                char c = s.charAt(i++);
                if (c == ']') return a;
            }
        }

        private String string() {
            i++; // opening quote
            StringBuilder sb = new StringBuilder();
            while (true) {
                char c = s.charAt(i++);
                if (c == '"') return sb.toString();
                if (c == '\\') {
                    char e = s.charAt(i++);
                    switch (e) {
                        case '"': sb.append('"'); break;
                        case '\\': sb.append('\\'); break;
                        case '/': sb.append('/'); break;
                        case 'b': sb.append('\b'); break;
                        case 'f': sb.append('\f'); break;
                        case 'n': sb.append('\n'); break;
                        case 'r': sb.append('\r'); break;
                        case 't': sb.append('\t'); break;
                        case 'u':
                            sb.append((char) Integer.parseInt(s.substring(i, i + 4), 16));
                            i += 4;
                            break;
                        default: sb.append(e);
                    }
                } else {
                    sb.append(c);
                }
            }
        }

        private JsonNumber number() {
            int start = i;
            while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
            return new JsonNumber(s.substring(start, i));
        }

        private void ws() {
            while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++;
        }
    }
}
