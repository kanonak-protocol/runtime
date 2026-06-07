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
    public static void main(String[] args) throws Exception {
        String vectors = args.length > 0
            ? args[0]
            : "../vectors/codec-vectors.json";

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

        System.out.println("\n" + passed + " passed, " + failed + " failed");
        System.exit(failed == 0 ? 0 : 1);
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
                    (String) p.get("datatype")));
            }
            classes.put(e.getKey(), new CodecClass((String) c.get("typeUri"), props));
        }
        return new CodecSchema(
            (String) s.get("typePredicate"),
            (String) s.get("labelPredicate"),
            (String) s.get("packageTypeUri"),
            classes);
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
