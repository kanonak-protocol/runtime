import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import org.kanonak.expression.Expression;
import org.kanonak.expression.Expression.ExpressionError;

/**
 * Drives the shared parity vectors through the Java kanonak-expression port. Every vector runs
 * through evaluate AND explain and their values must agree; ordered-comparison vectors supply
 * closures and refEnv, and vectors with a trace assert the verdict tree structurally.
 */
public final class Conformance {
    public static void main(String[] args) throws Exception {
        String vdir = args.length > 0 ? args[0] : "../vectors";
        int fails = run(Paths.get(vdir, "expression-vectors.json"));
        System.exit(fails == 0 ? 0 : 1);
    }

    /** The conformance context: numeric bindings (env) plus identity bindings (refEnv). */
    static final class Ctx {
        final Map<String, Object> env;
        final Map<String, Object> refEnv;
        Ctx(Map<String, Object> env, Map<String, Object> refEnv) { this.env = env; this.refEnv = refEnv; }
    }

    /** The conformance resolve hook: tx.VarRef -> env[varName] (raise if absent); any other unknown leaf -> raise. */
    static double resolve(Map<String, Object> node, Ctx ctx, Expression.Recurse<Ctx> evaluate) {
        if ("kanonak.org/transformations/VarRef".equals(node.get("type"))) {
            String name = (String) node.get("varName");
            if (ctx.env == null || !ctx.env.containsKey(name)) {
                throw new ExpressionError("unbound variable: " + name);
            }
            Object v = ctx.env.get(name);
            if (v instanceof Number n) return n.doubleValue();
            throw new ExpressionError("non-numeric binding for " + name + ": " + v);
        }
        throw new ExpressionError("unknown leaf node type: " + node.get("type"));
    }

    /** The identity-domain mirror: tx.VarRef -> refEnv[varName] member URI. */
    static String resolveRef(Map<String, Object> node, Ctx ctx) {
        if ("kanonak.org/transformations/VarRef".equals(node.get("type"))) {
            String name = (String) node.get("varName");
            if (ctx.refEnv == null || !ctx.refEnv.containsKey(name)) {
                throw new ExpressionError("unbound reference: " + name);
            }
            return (String) ctx.refEnv.get(name);
        }
        throw new ExpressionError("no reference resolver for leaf: " + node.get("type"));
    }

    /** The vector's closures JSON as the typed ClosureTable shape. */
    @SuppressWarnings("unchecked")
    static Map<String, Map<String, List<String>>> closuresOf(Map<String, Object> v) {
        Object raw = v.get("closures");
        if (!(raw instanceof Map<?, ?>)) return null;
        Map<String, Map<String, List<String>>> table = new java.util.LinkedHashMap<>();
        for (Map.Entry<String, Object> prop : ((Map<String, Object>) raw).entrySet()) {
            Map<String, List<String>> inner = new java.util.LinkedHashMap<>();
            for (Map.Entry<String, Object> member : ((Map<String, Object>) prop.getValue()).entrySet()) {
                List<String> reachable = new ArrayList<>();
                for (Object m : (List<Object>) member.getValue()) reachable.add((String) m);
                inner.put(member.getKey(), reachable);
            }
            table.put(prop.getKey(), inner);
        }
        return table;
    }

    /** Structural equality of a produced verdict tree against the vector's expected JSON tree. */
    @SuppressWarnings("unchecked")
    static boolean traceMatches(Expression.TraceNode got, Map<String, Object> want) {
        if (!got.type.equals(want.get("type"))) return false;
        if (got.value != ((Number) want.get("value")).doubleValue()) return false;
        if (!java.util.Objects.equals(got.leftRef, want.get("leftRef"))) return false;
        if (!java.util.Objects.equals(got.rightRef, want.get("rightRef"))) return false;
        List<Object> wantChildren = (List<Object>) want.getOrDefault("children", List.of());
        if (got.children.size() != wantChildren.size()) return false;
        for (int i = 0; i < wantChildren.size(); i++) {
            if (!traceMatches(got.children.get(i), (Map<String, Object>) wantChildren.get(i))) return false;
        }
        return true;
    }

    @SuppressWarnings("unchecked")
    static int run(Path path) throws Exception {
        Map<String, Object> doc = (Map<String, Object>) Json.parse(Files.readString(path, StandardCharsets.UTF_8));
        List<Object> vectors = (List<Object>) doc.get("vectors");
        int total = 0, pass = 0, fail = 0;
        for (Object o : vectors) {
            Map<String, Object> v = (Map<String, Object>) o;
            total++;
            String id = (String) v.get("id");
            Map<String, Object> expr = (Map<String, Object>) v.get("expr");
            Ctx ctx = new Ctx(
                (Map<String, Object>) v.getOrDefault("env", Map.of()),
                (Map<String, Object>) v.getOrDefault("refEnv", Map.of()));
            boolean expectError = Boolean.TRUE.equals(v.get("expectError"));
            Expression.EvalOptions<Ctx> options =
                new Expression.EvalOptions<>(closuresOf(v), Conformance::resolveRef);

            if (expectError) {
                boolean evalThrew = false, explainThrew = false;
                try { Expression.evaluate(expr, ctx, Conformance::resolve, options); } catch (RuntimeException e) { evalThrew = true; }
                try { Expression.explain(expr, ctx, Conformance::resolve, options); } catch (RuntimeException e) { explainThrew = true; }
                if (evalThrew && explainThrew) pass++;
                else { fail++; System.out.println("  FAIL [" + id + "] expected an error from evaluate AND explain"); }
                continue;
            }

            double got;
            Expression.TraceNode trace;
            try {
                got = Expression.evaluate(expr, ctx, Conformance::resolve, options);
                trace = Expression.explain(expr, ctx, Conformance::resolve, options);
            } catch (RuntimeException e) {
                fail++; System.out.println("  FAIL [" + id + "] threw: " + e.getMessage());
                continue;
            }
            double expected = ((Number) v.get("expected")).doubleValue();
            Object tol = v.get("tolerance");
            boolean ok = tol != null
                ? Math.abs(got - expected) <= ((Number) tol).doubleValue()
                : got == expected;
            if (!ok) {
                fail++; System.out.println("  FAIL [" + id + "] expected " + expected + ", got " + got);
                continue;
            }
            if (trace.value != got) {
                fail++; System.out.println("  FAIL [" + id + "] explain value " + trace.value + " != evaluate value " + got);
                continue;
            }
            Object wantTrace = v.get("trace");
            if (wantTrace != null && !traceMatches(trace, (Map<String, Object>) wantTrace)) {
                fail++; System.out.println("  FAIL [" + id + "] trace mismatch");
                continue;
            }
            pass++;
        }
        System.out.println("expression-vectors: " + pass + "/" + total + " pass" + (fail == 0 ? "" : ", " + fail + " fail"));
        return fail;
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
