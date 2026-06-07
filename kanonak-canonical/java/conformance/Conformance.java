import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import org.kanonak.canonical.Carrier;
import org.kanonak.canonical.Datatypes;
import org.kanonak.canonical.CanonicalForm;
import org.kanonak.canonical.CanonicalForm.Package;
import org.kanonak.canonical.CanonicalForm.Statement;
import org.kanonak.canonical.CanonicalForm.Subject;
import org.kanonak.canonical.CanonicalForm.Value;

/** Drives the shared golden vectors through the Java kanonak-canonical port. */
public final class Conformance {
    public static void main(String[] args) throws Exception {
        String vdir = args.length > 0 ? args[0] : "../vectors";
        int fails = 0;
        fails += runLexical(Paths.get(vdir, "lexical-vectors.json"));
        fails += runFullForm(Paths.get(vdir, "full-form-vectors.json"));
        System.out.println(fails == 0 ? "\nALL VECTORS PASS" : "\n" + fails + " VECTOR(S) FAILED");
        System.exit(fails == 0 ? 0 : 1);
    }

    @SuppressWarnings("unchecked")
    static int runLexical(Path path) throws Exception {
        Map<String, Object> doc = (Map<String, Object>) Json.parse(Files.readString(path, StandardCharsets.UTF_8));
        int total = 0, pass = 0, fail = 0;
        for (Object o : (List<Object>) doc.get("vectors")) {
            Map<String, Object> v = (Map<String, Object>) o;
            total++;
            String id = (String) v.get("id");
            Carrier carrier = Carrier.fromTag((String) v.get("carrier"));
            String input = (String) v.get("input");
            boolean expectError = Boolean.TRUE.equals(v.get("expectError"));
            try {
                String actual = Datatypes.canonicalScalarLexical(carrier, input);
                if (expectError) { fail++; System.out.println("  FAIL [" + id + "] expected error, got '" + actual + "'"); }
                else {
                    String expected = (String) v.get("expected");
                    if (actual.equals(expected)) pass++;
                    else { fail++; System.out.println("  FAIL [" + id + "] expected '" + expected + "', got '" + actual + "'"); }
                }
            } catch (RuntimeException e) {
                if (expectError) pass++;
                else { fail++; System.out.println("  FAIL [" + id + "] threw: " + e.getMessage()); }
            }
        }
        System.out.println("lexical-vectors: " + pass + "/" + total + " pass, " + fail + " fail");
        return fail;
    }

    @SuppressWarnings("unchecked")
    static int runFullForm(Path path) throws Exception {
        Map<String, Object> doc = (Map<String, Object>) Json.parse(Files.readString(path, StandardCharsets.UTF_8));
        int total = 0, pass = 0, fail = 0;
        for (Object o : (List<Object>) doc.get("vectors")) {
            Map<String, Object> v = (Map<String, Object>) o;
            total++;
            String id = (String) v.get("id");
            try {
                Package pkg = decodeSubjects((Map<String, Object>) v.get("input"));
                String form = CanonicalForm.serialize(pkg);
                String hash = CanonicalForm.hash(pkg);
                boolean ok = true;
                if (!form.equals(v.get("expectedCanonicalForm"))) {
                    ok = false;
                    System.out.println("  FAIL [" + id + "] form\n    expected: " + v.get("expectedCanonicalForm") + "\n    actual:   " + form);
                }
                if (!hash.equals(v.get("expectedHash"))) {
                    ok = false;
                    System.out.println("  FAIL [" + id + "] hash expected " + v.get("expectedHash") + " got " + hash);
                }
                if (ok) pass++; else fail++;
            } catch (RuntimeException e) {
                fail++; System.out.println("  FAIL [" + id + "] threw: " + e.getMessage());
            }
        }
        System.out.println("full-form-vectors: " + pass + "/" + total + " pass, " + fail + " fail");
        return fail;
    }

    @SuppressWarnings("unchecked")
    static Package decodeSubjects(Map<String, Object> input) {
        List<Subject> subjects = new ArrayList<>();
        for (Object o : (List<Object>) input.get("subjects")) {
            Map<String, Object> s = (Map<String, Object>) o;
            subjects.add(new Subject((String) s.get("uri"), decodeStatements(s)));
        }
        return new Package(subjects);
    }

    @SuppressWarnings("unchecked")
    static List<Statement> decodeStatements(Map<String, Object> node) {
        List<Statement> out = new ArrayList<>();
        Object stmts = node.get("statements");
        if (stmts instanceof List<?> list) {
            for (Object o : list) {
                Map<String, Object> st = (Map<String, Object>) o;
                out.add(new Statement((String) st.get("predicate"), decodeValue((Map<String, Object>) st.get("value"))));
            }
        }
        return out;
    }

    @SuppressWarnings("unchecked")
    static Value decodeValue(Map<String, Object> v) {
        if (v.containsKey("lit")) {
            String lit = (String) v.get("lit");
            Carrier c = Carrier.of((String) v.get("datatype"));
            return c != null ? new Value.Typed(c, lit) : new Value.Raw(lit);
        }
        if (v.containsKey("raw")) return new Value.Raw((String) v.get("raw"));
        if (v.containsKey("ref")) return new Value.Ref((String) v.get("ref"));
        if (v.containsKey("embed")) {
            Map<String, Object> emb = (Map<String, Object>) v.get("embed");
            return new Value.Embed((String) emb.get("name"), decodeStatements(emb));
        }
        if (v.containsKey("list")) {
            List<Value> items = new ArrayList<>();
            for (Object o : (List<Object>) v.get("list")) items.add(decodeValue((Map<String, Object>) o));
            return new Value.KList(items);
        }
        throw new IllegalStateException("decode: unknown value shape " + v);
    }

    // -- Minimal JSON parser (objects/arrays/strings/numbers/booleans/null) ----

    static final class Json {
        private final String s;
        private int i;

        private Json(String s) { this.s = s; }

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
            Map<String, Object> m = new java.util.LinkedHashMap<>();
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

        private Double number() {
            int start = i;
            while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
            return Double.parseDouble(s.substring(start, i));
        }

        private void ws() {
            while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++;
        }
    }
}
