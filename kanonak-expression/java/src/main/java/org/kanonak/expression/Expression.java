package org.kanonak.expression;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;
import java.util.regex.PatternSyntaxException;

/**
 * Kanonak expression runtime (expressionRuntimeVersion "2") — a small, deterministic
 * tree-walker that folds a {@code kanonak.org/transformations} (tx) + {@code kanonak.org/math}
 * expression tree to a VALUE. An independent conformant Java port of the reference kernel,
 * verified against the shared parity vectors. Standard library only.
 *
 * <p>VALUE DOMAIN (v2): {@code number | string | ref | list} — represented as
 * {@code Double | String | Ref | List<Object>}. Booleans and comparison results remain
 * {@code 1.0}/{@code 0.0}; a {@link Ref} is a canonical versionless URI identity (distinct
 * from String so Equals holds the cross-kind-is-false rule); ABSENCE IS THE EMPTY LIST —
 * there is no null value. The caller's resolve may return any value: a property read is just
 * a caller leaf that returns a list. The kernel NEVER touches a graph.
 *
 * <p>ERROR CONTRACT: computations fail LOUD (arithmetic on a non-number, an aggregate over
 * non-numeric elements, Min/Max/Average on empty, a nested list in Join, an out-of-subset
 * Matches pattern); predicates fail CLOSED (Equals cross-kind and the ordering comparisons
 * on non-numbers yield 0).
 *
 * <p>LAMBDA BINDING: Filter/ListMap/ForEach bind their loopVar per element; within their
 * bodies — and only there — a tx.VarRef naming a lexically-enclosing loopVar is resolved by
 * the kernel (innermost binder wins). Recursion re-entered from inside resolve carries no
 * frames.
 *
 * <p>MATCHES: the pinned RE2-compatible XSD-regex subset. Java's Pattern counts code points
 * for quantifiers and {@code .} natively; the dialect translations THIS engine owes:
 * {@code \v} (a vertical-whitespace CLASS here) becomes {@code \x0B}; non-dotAll {@code .}
 * (which natively excludes more line terminators than the pinned {@code [^\n]}) translates
 * to {@code [^\n]}; case folding adds {@code UNICODE_CASE} (the pin is Unicode simple
 * folding; Java's default is ASCII). {@code \b}/{@code \d}/{@code \w}/{@code \s} are ASCII
 * natively, and the shorthand expansions apply uniformly anyway.
 *
 * <p>A change to any primitive, value rule, or dispatch entry requires a NEW
 * EXPRESSION_RUNTIME_VERSION, never an edit in place.
 */
public final class Expression {
    private Expression() {}

    /** The frozen expression-runtime version (determinism contract). Not hashed. */
    public static final String EXPRESSION_RUNTIME_VERSION = "2";

    private static final String TX = "kanonak.org/transformations";
    private static final String MATH = "kanonak.org/math";

    /** A reference value — a member's canonical versionless URI identity. */
    public static final class Ref {
        public final String uri;
        public Ref(String uri) { this.uri = uri; }
        @Override public boolean equals(Object o) { return o instanceof Ref && ((Ref) o).uri.equals(uri); }
        @Override public int hashCode() { return uri.hashCode(); }
        @Override public String toString() { return "Ref(" + uri + ")"; }
    }

    /** Resolve any node the kernel does not recognise — a binding, a host graph read
     * (a property-read leaf returning a list), or a domain leaf — to a value. */
    @FunctionalInterface
    public interface Resolve<C> {
        Object resolve(Map<String, Object> node, C ctx, Recurse<C> evaluate);
    }

    /** The kernel's evaluate handed back to a {@link Resolve} so domain leaves can recurse
     * (WITHOUT lambda frames — the caller's subtrees are the caller's scope). */
    @FunctionalInterface
    public interface Recurse<C> {
        Object apply(Map<String, Object> node, C ctx);
    }

    /** Resolve an identity leaf inside an ordered comparison. */
    @FunctionalInterface
    public interface ResolveRef<C> {
        String resolve(Map<String, Object> node, C ctx);
    }

    /** Optional evaluation context for the ordered comparisons. */
    public static final class EvalOptions<C> {
        final Map<String, Map<String, List<String>>> closures;
        final ResolveRef<C> resolveRef;
        public EvalOptions(Map<String, Map<String, List<String>>> closures, ResolveRef<C> resolveRef) {
            this.closures = closures;
            this.resolveRef = resolveRef;
        }
    }

    /** One node of an evaluation trace — the verdict tree {@link #explain} returns.
     * A runtime return shape, not an ontology class. */
    public static final class TraceNode {
        public final String type;
        public final Object value;
        public final List<TraceNode> children;
        public final String leftRef;
        public final String rightRef;
        TraceNode(String type, Object value, List<TraceNode> children, String leftRef, String rightRef) {
            this.type = type;
            this.value = value;
            this.children = children;
            this.leftRef = leftRef;
            this.rightRef = rightRef;
        }
    }

    /** The runtime error type. */
    public static final class ExpressionError extends RuntimeException {
        public ExpressionError(String msg) { super(msg); }
    }

    private static ExpressionError err(String msg) { return new ExpressionError(msg); }

    // -- helpers --------------------------------------------------------------

    private static String kindOf(Object v) {
        if (v instanceof Double) return "number";
        if (v instanceof String) return "string";
        if (v instanceof Ref) return "ref";
        if (v instanceof List) return "list";
        return "unknown";
    }

    private static double requireNum(Object v, String op) {
        if (!(v instanceof Double)) throw err(op + " requires a numeric operand, got " + kindOf(v));
        return (Double) v;
    }

    private static boolean truthy(double n) { return n != 0.0; }
    private static double bool(boolean b) { return b ? 1.0 : 0.0; }

    private static void requireDomain(boolean ok, String msg) { if (!ok) throw err(msg); }

    private static double flooredMod(double a, double b) {
        requireDomain(b != 0.0, "Modulo by zero");
        return a - b * Math.floor(a / b);
    }

    private static double roundHalfAway(double a) {
        return a < 0 ? -Math.floor(-a + 0.5) : Math.floor(a + 0.5);
    }

    /** Polymorphic tx.Equals: scalars by value, refs by URI identity, lists never equal,
     * cross-kind false. Never errors. */
    private static boolean valuesEqual(Object a, Object b) {
        if (a instanceof Double && b instanceof Double) return a.equals(b);
        if (a instanceof String && b instanceof String) return a.equals(b);
        if (a instanceof Ref && b instanceof Ref) return a.equals(b);
        return false;
    }

    private static String typeOf(Map<String, Object> node) {
        Object t = node.get("type");
        if (!(t instanceof String)) throw err("Expression node has no 'type'");
        return (String) t;
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> operand(Map<String, Object> node, String typ, String key) {
        Object v = node.get(key);
        if (!(v instanceof Map)) throw err(typ + " is missing operand '" + key + "'");
        return (Map<String, Object>) v;
    }

    private static double toNumber(Object v) {
        if (v instanceof Number) return ((Number) v).doubleValue();
        if (v instanceof String) {
            try { return Double.parseDouble((String) v); } catch (NumberFormatException e) { throw err("not a number: " + v); }
        }
        throw err("not a number: " + v);
    }

    /** A literal node's value, or null when not a literal (the domain has no null values,
     * so null is safe as the not-a-literal sentinel). */
    private static Object literalValue(Map<String, Object> node, String typ) {
        switch (typ) {
            case TX + "/IntegerLiteral": return toNumber(node.get("integerLiteral"));
            case TX + "/DecimalLiteral": return toNumber(node.get("decimalLiteral"));
            case TX + "/BooleanLiteral": {
                Object b = node.get("booleanLiteral");
                return bool(Boolean.TRUE.equals(b) || "true".equals(b));
            }
            case TX + "/StringLiteral": {
                Object s = node.get("stringLiteral");
                if (!(s instanceof String)) throw err("StringLiteral is missing stringLiteral");
                return s;
            }
            case TX + "/UriLiteral": {
                Object s = node.get("refTo");
                if (!(s instanceof String) || ((String) s).isEmpty()) throw err("UriLiteral is missing refTo");
                return new Ref((String) s);
            }
            default: return null;
        }
    }

    /** ECMAScript-style number-to-string (the RFC 8785 rule): integral without a point. */
    private static String formatNumber(double n) {
        if (n == Math.floor(n) && Math.abs(n) < 1e21 && !Double.isInfinite(n)) {
            return Long.toString((long) n);
        }
        return Double.toString(n);
    }

    private static String joinElement(Object v) {
        if (v instanceof String) return (String) v;
        if (v instanceof Double) return formatNumber((Double) v);
        if (v instanceof Ref) {
            String uri = ((Ref) v).uri;
            int i = uri.lastIndexOf('/');
            return i >= 0 ? uri.substring(i + 1) : uri;
        }
        throw err("Join cannot stringify a nested list");
    }

    private static boolean isSet(Object v) {
        if (v instanceof String) return !((String) v).isEmpty();
        if (v instanceof List) return !((List<?>) v).isEmpty();
        return true;
    }

    // -- Matches: the pinned RE2-compatible XSD-regex subset ------------------

    private static final Pattern FLAG_PREFIX = Pattern.compile("^\\(\\?([ims]+)\\)");
    private static final Pattern QUANTIFIER = Pattern.compile("^\\{\\d+(,\\d*)?\\}");
    private static final String ALLOWED_ESCAPES = "dDwWsSbBnrtfv.*+?()[]{}|^$\\/";

    /** The same subset scanner as the reference kernel. */
    /**
     * Check a WHOLE pattern against the pinned subset, flag prefix included.
     * Thin wrapper over {@link #parseMatchesPattern} so this checker and the
     * evaluator can never disagree about what is a valid pattern.
     */
    static void validateMatchesPattern(String pattern) {
        parseMatchesPattern(pattern);
    }

    /** A whole pattern decomposed into its flag prefix and body. */
    private record MatchesPattern(String flags, String body) {}

    /**
     * THE definition of a valid {@code Matches} pattern: the single place that
     * decides what the pinned subset accepts AND how a whole pattern decomposes
     * into its flag prefix and body. Every diagnostic quotes the WHOLE pattern
     * the caller passed, never the flag-stripped body.
     */
    private static MatchesPattern parseMatchesPattern(String pattern) {
        // Whole-pattern flag prefix - position 0 only, over i/m/s, EACH AT MOST
        // ONCE. The repeat rule is the subset's, not the host engine's: the JS
        // engine rejects (?ii) at compile time while the Go, Python and Rust
        // engines accept it, so the subset decides rather than the host.
        String flags = "";
        String body = pattern;
        java.util.regex.Matcher prefix = FLAG_PREFIX.matcher(pattern);
        if (prefix.find()) {
            flags = prefix.group(1);
            java.util.Set<Character> seen = new java.util.HashSet<>();
            for (char f : flags.toCharArray()) {
                if (!seen.add(f)) throw subsetErr("repeated flag in prefix (?" + flags + ")", pattern);
            }
            body = pattern.substring(prefix.end());
        }
        boolean inClass = false;
        int n = body.length();
        for (int i = 0; i < n; i++) {
            char c = body.charAt(i);
            if (c == '\\') {
                if (i + 1 >= n) throw subsetErr("trailing backslash", pattern);
                char e = body.charAt(i + 1);
                if (e == 'x') {
                    if (i + 2 < n && body.charAt(i + 2) == '{') throw subsetErr("\\x{…} escape", pattern);
                    if (i + 3 >= n || !isHex(body.charAt(i + 2)) || !isHex(body.charAt(i + 3))) {
                        throw subsetErr("\\x escape must be \\xHH", pattern);
                    }
                    i += 3;
                    continue;
                }
                if (e >= '0' && e <= '9') throw subsetErr("backreference or octal escape \\" + e, pattern);
                if (e == 'p' || e == 'P') throw subsetErr("unicode property class \\" + e + "{…}", pattern);
                if (e == 'k') throw subsetErr("named backreference \\k", pattern);
                if (e == 'u') throw subsetErr("\\u escape", pattern);
                if (e == '-') {
                    if (!inClass) throw subsetErr("\\- outside a character class", pattern);
                    i++;
                    continue;
                }
                if ((e == 'b' || e == 'B' || e == 'D' || e == 'W' || e == 'S') && inClass) {
                    throw subsetErr("\\" + e + " inside a character class", pattern);
                }
                if (ALLOWED_ESCAPES.indexOf(e) < 0) throw subsetErr("escape \\" + e, pattern);
                i++;
                continue;
            }
            if (inClass) {
                if (c == ']') inClass = false;
                else if (c == '&' && i + 1 < n && body.charAt(i + 1) == '&') throw subsetErr("character-class intersection &&", pattern);
                else if (c == '[' && i + 1 < n && body.charAt(i + 1) == ':') throw subsetErr("POSIX class [[:…:]]", pattern);
                continue;
            }
            if (c == '[') { inClass = true; continue; }
            if (c == '(' && i + 1 < n && body.charAt(i + 1) == '?') {
                if (i + 2 >= n || body.charAt(i + 2) != ':') throw subsetErr("group construct (?", pattern);
                i += 2;
                continue;
            }
            if (c == '{') {
                // A bare `{` must start a valid quantifier — the SCANNER enforces
                // this uniformly (a literal brace is written \{).
                if (!QUANTIFIER.matcher(body.substring(i)).find()) {
                    throw subsetErr("bare '{' that is not a quantifier (write \\{)", pattern);
                }
            }
        }
        if (inClass) throw subsetErr("unterminated character class", pattern);
        return new MatchesPattern(flags, body);
    }

    private static ExpressionError subsetErr(String what, String pattern) {
        return err("Matches pattern is outside the pinned regex subset (" + what + "): " + pattern);
    }

    private static boolean isHex(char c) {
        return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');
    }

    // The ASCII word-boundary lookarounds. Java's native \b uses a UNICODE
    // word definition on some JDKs (temurin-17 matched é as a word char while
    // JDK 22 did not — the conformance vectors caught the JDK-version
    // divergence), so \b/\B expand to explicit ASCII-word-class lookarounds,
    // which are JDK-independent. Host lookaround in a port's COMPILED form is
    // fine — the subset restriction is on what authors write.
    private static final String WORD = "[0-9A-Za-z_]";
    private static final String B_BOUNDARY =
        "(?:(?<=" + WORD + ")(?!" + WORD + ")|(?<!" + WORD + ")(?=" + WORD + "))";
    private static final String B_NON_BOUNDARY =
        "(?:(?<=" + WORD + ")(?=" + WORD + ")|(?<!" + WORD + ")(?!" + WORD + "))";

    /** The pinned ASCII expansions, plus THIS engine's owed translations:
     * {@code \v} → {@code \x0B} (Java's \v is a class), non-dotAll {@code .} →
     * {@code [^\n]} (Java's . excludes more line terminators than the pin), and
     * {@code \b}/{@code \B} → the ASCII-boundary lookarounds above. */
    private static String expandShorthandClasses(String body, boolean dotAll) {
        StringBuilder out = new StringBuilder(body.length() + 16);
        boolean inClass = false;
        int n = body.length();
        for (int i = 0; i < n; i++) {
            char c = body.charAt(i);
            if (c == '\\') {
                char e = body.charAt(i + 1); // subset-validated: never trailing
                String expansion = null;
                if (inClass) {
                    switch (e) {
                        case 'd': expansion = "0-9"; break;
                        case 'w': expansion = "0-9A-Za-z_"; break;
                        case 's': expansion = " \\t\\n\\r\\f\\x0B"; break;
                        case 'v': expansion = "\\x0B"; break;
                        default: break;
                    }
                } else {
                    switch (e) {
                        case 'd': expansion = "[0-9]"; break;
                        case 'D': expansion = "[^0-9]"; break;
                        case 'w': expansion = "[0-9A-Za-z_]"; break;
                        case 'W': expansion = "[^0-9A-Za-z_]"; break;
                        case 's': expansion = "[ \\t\\n\\r\\f\\x0B]"; break;
                        case 'S': expansion = "[^ \\t\\n\\r\\f\\x0B]"; break;
                        case 'v': expansion = "\\x0B"; break;
                        case 'b': expansion = B_BOUNDARY; break;
                        case 'B': expansion = B_NON_BOUNDARY; break;
                        default: break;
                    }
                }
                if (expansion != null) out.append(expansion);
                else out.append(c).append(e);
                i++;
                continue;
            }
            if (!inClass && c == '.' && !dotAll) {
                out.append("[^\\n]");
                continue;
            }
            if (!inClass && c == '[') inClass = true;
            else if (inClass && c == ']') inClass = false;
            out.append(c);
        }
        return out.toString();
    }

    /** fn:matches semantics: UNANCHORED. Java's Pattern counts code points natively;
     * case folding adds UNICODE_CASE per the pinned Unicode simple folding. */
    private static boolean matchesPattern(String input, String pattern) {
        MatchesPattern parsed = parseMatchesPattern(pattern);
        String flags = parsed.flags();
        String body = parsed.body();
        boolean dotAll = flags.indexOf('s') >= 0;
        int f = 0;
        if (flags.indexOf('i') >= 0) f |= Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE;
        if (flags.indexOf('m') >= 0) f |= Pattern.MULTILINE;
        if (dotAll) f |= Pattern.DOTALL;
        Pattern compiled;
        try {
            compiled = Pattern.compile(expandShorthandClasses(body, dotAll), f);
        } catch (PatternSyntaxException e) {
            throw err("Matches pattern does not compile: " + pattern);
        }
        return compiled.matcher(input).find();
    }

    // -- ordered comparisons (unchanged from v1) ------------------------------

    private static <C> String identityOf(Map<String, Object> node, C ctx, EvalOptions<C> opts) {
        String typ = typeOf(node);
        if (typ.equals(TX + "/UriLiteral")) {
            Object s = node.get("refTo");
            if (!(s instanceof String) || ((String) s).isEmpty()) throw err("UriLiteral is missing refTo");
            return (String) s;
        }
        if (opts == null || opts.resolveRef == null) {
            throw err("No resolveRef supplied for identity leaf '" + typ + "'");
        }
        return opts.resolveRef.resolve(node, ctx);
    }

    private static <C> Object[] foldOrdered(Map<String, Object> node, String typ, C ctx, EvalOptions<C> opts) {
        Object viaRaw = node.get("viaProperty");
        if (!(viaRaw instanceof String) || ((String) viaRaw).isEmpty()) throw err(typ + " is missing viaProperty");
        String via = (String) viaRaw;
        String left = identityOf(operand(node, typ, "compareLeft"), ctx, opts);
        String right = identityOf(operand(node, typ, "compareRight"), ctx, opts);
        Map<String, List<String>> closure = (opts == null || opts.closures == null) ? null : opts.closures.get(via);
        if (closure == null) throw err("No closure supplied for ordering property '" + via + "'");
        double value;
        if (left.equals(right)) {
            value = bool(typ.equals(TX + "/IsAtLeast"));
        } else {
            List<String> reach = closure.get(left);
            value = bool(reach != null && reach.contains(right));
        }
        return new Object[] { value, left, right };
    }

    // -- dispatch -------------------------------------------------------------

    private static final class Arity {
        final String kind; final String a; final String b; final String c;
        Arity(String kind, String a, String b, String c) { this.kind = kind; this.a = a; this.b = b; this.c = c; }
    }

    private static final Arity ARITH = new Arity("binary", "arithLeft", "arithRight", null);
    private static final Arity COMPARE = new Arity("binary", "compareLeft", "compareRight", null);
    private static final Arity VALUE = new Arity("unary", "value", null, null);

    private static Arity operatorArity(String typ) {
        switch (typ) {
            case TX + "/Add": case TX + "/Subtract": case TX + "/Multiply": case TX + "/Divide":
            case MATH + "/Power": case MATH + "/Modulo": case MATH + "/Minimum": case MATH + "/Maximum":
                return ARITH;
            case TX + "/Abs": case TX + "/Negate": case MATH + "/Exp": case MATH + "/Ln":
            case MATH + "/Log10": case MATH + "/Sqrt": case MATH + "/Floor": case MATH + "/Ceil":
            case MATH + "/Round": case MATH + "/Sign":
                return VALUE;
            case TX + "/Equals": case TX + "/GreaterThan": case TX + "/LessThan":
            case TX + "/GreaterThanOrEqual": case TX + "/LessThanOrEqual":
                return COMPARE;
            case TX + "/And": case TX + "/Or":
                return new Arity("nary", "operands", null, null);
            case MATH + "/Clip":
                return new Arity("ternary", "clipValue", "clipLower", "clipUpper");
            default:
                return null;
        }
    }

    private static double unary(String typ, double x) {
        switch (typ) {
            case TX + "/Abs": return Math.abs(x);
            case TX + "/Negate": return -x;
            case MATH + "/Exp": return Math.exp(x);
            case MATH + "/Ln": requireDomain(x > 0, "Ln of a non-positive number"); return Math.log(x);
            case MATH + "/Log10": requireDomain(x > 0, "Log10 of a non-positive number"); return Math.log10(x);
            case MATH + "/Sqrt": requireDomain(x >= 0, "Sqrt of a negative number"); return Math.sqrt(x);
            case MATH + "/Floor": return Math.floor(x);
            case MATH + "/Ceil": return Math.ceil(x);
            case MATH + "/Round": return roundHalfAway(x);
            case MATH + "/Sign": return Math.signum(x);
            default: throw err("No unary primitive for " + typ);
        }
    }

    private static double binaryArith(String typ, double a, double b) {
        switch (typ) {
            case TX + "/Add": return a + b;
            case TX + "/Subtract": return a - b;
            case TX + "/Multiply": return a * b;
            case TX + "/Divide": requireDomain(b != 0, "Divide by zero"); return a / b;
            case MATH + "/Power": return Math.pow(a, b);
            case MATH + "/Modulo": return flooredMod(a, b);
            case MATH + "/Minimum": return Math.min(a, b);
            case MATH + "/Maximum": return Math.max(a, b);
            default: throw err("No arithmetic primitive for " + typ);
        }
    }

    private static boolean isOrderComparison(String typ) {
        return typ.equals(TX + "/GreaterThan") || typ.equals(TX + "/LessThan")
            || typ.equals(TX + "/GreaterThanOrEqual") || typ.equals(TX + "/LessThanOrEqual");
    }

    private static double binaryOrder(String typ, double a, double b) {
        switch (typ) {
            case TX + "/GreaterThan": return bool(a > b);
            case TX + "/LessThan": return bool(a < b);
            case TX + "/GreaterThanOrEqual": return bool(a >= b);
            case TX + "/LessThanOrEqual": return bool(a <= b);
            default: throw err("No ordering primitive for " + typ);
        }
    }

    private static String iteratorBody(String typ) {
        switch (typ) {
            case TX + "/ForEach": return "emit";
            case TX + "/ListMap": return "mapBody";
            case TX + "/Filter": return "predicate";
            default: return null;
        }
    }

    private static boolean isListFold(String typ) {
        switch (typ) {
            case TX + "/Count": case TX + "/Sum": case TX + "/Min": case TX + "/Max":
            case TX + "/Average": case TX + "/Join": case TX + "/Reverse":
                return true;
            default: return false;
        }
    }

    @SuppressWarnings("unchecked")
    private static Object listFold(String typ, List<Object> items, Map<String, Object> node) {
        String name = typ.substring(typ.lastIndexOf('/') + 1);
        switch (typ) {
            case TX + "/Count": return (double) items.size();
            case TX + "/Sum": {
                double total = 0;
                for (Object el : items) total += requireNum(el, "Sum");
                return total;
            }
            case TX + "/Min": case TX + "/Max": {
                if (items.isEmpty()) throw err(name + " on an empty list is undefined; guard with IsSet");
                double best = requireNum(items.get(0), name);
                for (Object el : items.subList(1, items.size())) {
                    double v = requireNum(el, name);
                    if ((name.equals("Min") && v < best) || (name.equals("Max") && v > best)) best = v;
                }
                return best;
            }
            case TX + "/Average": {
                if (items.isEmpty()) throw err("Average on an empty list is undefined; guard with IsSet");
                double total = 0;
                for (Object el : items) total += requireNum(el, "Average");
                return total / items.size();
            }
            case TX + "/Join": {
                Object sepRaw = node.get("separator");
                String sep = sepRaw instanceof String ? (String) sepRaw : "";
                StringBuilder sb = new StringBuilder();
                for (int i = 0; i < items.size(); i++) {
                    if (i > 0) sb.append(sep);
                    sb.append(joinElement(items.get(i)));
                }
                return sb.toString();
            }
            case TX + "/Reverse": {
                List<Object> out = new ArrayList<>(items);
                java.util.Collections.reverse(out);
                return out;
            }
            default: throw err("No list fold for " + typ);
        }
    }

    private static Boolean kindPredicate(String typ, Object v) {
        switch (typ) {
            case TX + "/IsString": return v instanceof String;
            case TX + "/IsNumber": return v instanceof Double;
            case TX + "/IsReference": return v instanceof Ref;
            case TX + "/IsList": return v instanceof List;
            default: return null;
        }
    }

    private static boolean isKindPredicate(String typ) {
        return typ.equals(TX + "/IsString") || typ.equals(TX + "/IsNumber")
            || typ.equals(TX + "/IsReference") || typ.equals(TX + "/IsList");
    }

    // -- the fold -------------------------------------------------------------

    private static final class Frame {
        final String name; final Object value;
        Frame(String name, Object value) { this.name = name; this.value = value; }
    }

    private static Object boundValue(List<Frame> frames, String name) {
        for (int i = frames.size() - 1; i >= 0; i--) {
            if (frames.get(i).name.equals(name)) return frames.get(i).value;
        }
        return null;
    }

    /** Evaluate an expression tree to a value (no ordered-comparison context). */
    public static <C> Object evaluate(Map<String, Object> node, C ctx, Resolve<C> resolve) {
        return evaluate(node, ctx, resolve, null);
    }

    /** Evaluate with ordered-comparison context. */
    public static <C> Object evaluate(Map<String, Object> node, C ctx, Resolve<C> resolve, EvalOptions<C> options) {
        return go(node, ctx, resolve, options, new ArrayList<>());
    }

    @SuppressWarnings("unchecked")
    private static <C> List<Object> sourceList(Map<String, Object> node, String typ, C ctx, Resolve<C> resolve,
                                               EvalOptions<C> options, List<Frame> frames) {
        Object v = go(operand(node, typ, "source"), ctx, resolve, options, frames);
        if (v instanceof List) return (List<Object>) v;
        List<Object> one = new ArrayList<>(1);
        one.add(v);
        return one;
    }

    @SuppressWarnings("unchecked")
    private static <C> Object go(Map<String, Object> node, C ctx, Resolve<C> resolve,
                                 EvalOptions<C> options, List<Frame> frames) {
        String typ = typeOf(node);

        Arity ar = operatorArity(typ);
        if (ar != null) {
            switch (ar.kind) {
                case "unary": {
                    Object x = go(operand(node, typ, ar.a), ctx, resolve, options, frames);
                    return unary(typ, requireNum(x, typ));
                }
                case "binary": {
                    Object a = go(operand(node, typ, ar.a), ctx, resolve, options, frames);
                    Object b = go(operand(node, typ, ar.b), ctx, resolve, options, frames);
                    if (typ.equals(TX + "/Equals")) return bool(valuesEqual(a, b));
                    if (isOrderComparison(typ)) {
                        // Predicate: non-numeric operands fail CLOSED.
                        if (!(a instanceof Double) || !(b instanceof Double)) return 0.0;
                        return binaryOrder(typ, (Double) a, (Double) b);
                    }
                    return binaryArith(typ, requireNum(a, typ), requireNum(b, typ));
                }
                case "nary": {
                    Object itemsRaw = node.get(ar.a);
                    if (!(itemsRaw instanceof List)) throw err(typ + " expects an '" + ar.a + "' list");
                    boolean isAnd = typ.equals(TX + "/And");
                    for (Object item : (List<Object>) itemsRaw) {
                        boolean v = truthy(requireNum(go((Map<String, Object>) item, ctx, resolve, options, frames), typ));
                        if (isAnd && !v) return 0.0;
                        if (!isAnd && v) return 1.0;
                    }
                    return bool(isAnd);
                }
                default: { // ternary — only Clip today.
                    double v = requireNum(go(operand(node, typ, ar.a), ctx, resolve, options, frames), typ);
                    double lo = requireNum(go(operand(node, typ, ar.b), ctx, resolve, options, frames), typ);
                    double hi = requireNum(go(operand(node, typ, ar.c), ctx, resolve, options, frames), typ);
                    return Math.min(Math.max(v, lo), hi);
                }
            }
        }

        if (typ.equals(TX + "/Not")) {
            Object x = go(operand(node, typ, "operand"), ctx, resolve, options, frames);
            return bool(!truthy(requireNum(x, typ)));
        }

        if (typ.equals(TX + "/IsAtLeast") || typ.equals(TX + "/Dominates")) {
            return foldOrdered(node, typ, ctx, options)[0];
        }

        if (isListFold(typ)) {
            return listFold(typ, sourceList(node, typ, ctx, resolve, options, frames), node);
        }

        String bodyKey = iteratorBody(typ);
        if (bodyKey != null) {
            Object loopVarRaw = node.get("loopVar");
            if (!(loopVarRaw instanceof String) || ((String) loopVarRaw).isEmpty()) throw err(typ + " is missing loopVar");
            String loopVar = (String) loopVarRaw;
            List<Object> items = sourceList(node, typ, ctx, resolve, options, frames);
            Map<String, Object> body = operand(node, typ, bodyKey);
            List<Object> out = new ArrayList<>();
            for (Object el : items) {
                frames.add(new Frame(loopVar, el));
                Object v;
                try {
                    v = go(body, ctx, resolve, options, frames);
                } finally {
                    frames.remove(frames.size() - 1);
                }
                if (typ.equals(TX + "/Filter")) {
                    if (truthy(requireNum(v, "Filter predicate"))) out.add(el);
                } else if (typ.equals(TX + "/ForEach")) {
                    // Flatten one level; an empty list contributes nothing.
                    if (v instanceof List) out.addAll((List<Object>) v);
                    else out.add(v);
                } else {
                    out.add(v);
                }
            }
            return out;
        }

        if (typ.equals(TX + "/Contains")) {
            Object hay = go(operand(node, typ, "haystack"), ctx, resolve, options, frames);
            Object needle = go(operand(node, typ, "needle"), ctx, resolve, options, frames);
            List<Object> items = hay instanceof List ? (List<Object>) hay : java.util.Collections.singletonList(hay);
            for (Object el : items) {
                if (valuesEqual(el, needle)) return 1.0;
            }
            return 0.0;
        }

        if (typ.equals(TX + "/IsSet")) {
            return bool(isSet(go(operand(node, typ, "checkExpr"), ctx, resolve, options, frames)));
        }

        if (typ.equals(TX + "/ListItemAt")) {
            List<Object> items = sourceList(node, typ, ctx, resolve, options, frames);
            Object idx = go(operand(node, typ, "itemIndex"), ctx, resolve, options, frames);
            if (!(idx instanceof Double) || (Double) idx != Math.floor((Double) idx) || (Double) idx < 0) {
                throw err("ListItemAt itemIndex must be a non-negative integer");
            }
            int i = (int) (double) (Double) idx;
            // Past the end is ABSENCE (the empty list); guard with IsSet.
            return i < items.size() ? items.get(i) : new ArrayList<>();
        }

        if (typ.equals(TX + "/Matches")) {
            Object src = go(operand(node, typ, "matchSource"), ctx, resolve, options, frames);
            if (!(src instanceof String)) throw err("Matches requires a string matchSource, got " + kindOf(src));
            Object pattern = node.get("pattern");
            if (!(pattern instanceof String)) throw err("Matches is missing pattern");
            return bool(matchesPattern((String) src, (String) pattern));
        }

        if (isKindPredicate(typ)) {
            Object x = go(operand(node, typ, "kindCheck"), ctx, resolve, options, frames);
            return bool(kindPredicate(typ, x));
        }

        Object lit = literalValue(node, typ);
        if (lit != null) return lit;

        // A VarRef naming a lexically-enclosing loopVar is the kernel's own bound
        // variable — the ONLY leaf the kernel answers. Everything else is the
        // caller's; recursion from inside resolve re-enters WITHOUT frames.
        if (typ.equals(TX + "/VarRef")) {
            Object name = node.get("varName");
            if (name instanceof String) {
                Object bound = boundValue(frames, (String) name);
                if (bound != null) return bound;
            }
        }

        return resolve.resolve(node, ctx, (n, c) -> go(n, c, resolve, options, new ArrayList<>()));
    }

    // -- explain --------------------------------------------------------------

    /** Evaluate and return the verdict tree. The root's value is exactly what
     * {@link #evaluate} returns for the same inputs; the conformance suite runs
     * every vector through both and requires agreement. */
    public static <C> TraceNode explain(Map<String, Object> node, C ctx, Resolve<C> resolve, EvalOptions<C> options) {
        return trace(node, ctx, resolve, options, new ArrayList<>());
    }

    private static TraceNode leaf(String typ, Object value) {
        return new TraceNode(typ, value, new ArrayList<>(), null, null);
    }

    private static TraceNode parent(String typ, Object value, List<TraceNode> children) {
        return new TraceNode(typ, value, children, null, null);
    }

    @SuppressWarnings("unchecked")
    private static <C> TraceNode trace(Map<String, Object> node, C ctx, Resolve<C> resolve,
                                       EvalOptions<C> options, List<Frame> frames) {
        String typ = typeOf(node);

        Arity ar = operatorArity(typ);
        if (ar != null) {
            switch (ar.kind) {
                case "unary": {
                    TraceNode x = trace(operand(node, typ, ar.a), ctx, resolve, options, frames);
                    return parent(typ, unary(typ, requireNum(x.value, typ)), listOf(x));
                }
                case "binary": {
                    TraceNode a = trace(operand(node, typ, ar.a), ctx, resolve, options, frames);
                    TraceNode b = trace(operand(node, typ, ar.b), ctx, resolve, options, frames);
                    Object value;
                    if (typ.equals(TX + "/Equals")) {
                        value = bool(valuesEqual(a.value, b.value));
                    } else if (isOrderComparison(typ)) {
                        if (!(a.value instanceof Double) || !(b.value instanceof Double)) value = 0.0;
                        else value = binaryOrder(typ, (Double) a.value, (Double) b.value);
                    } else {
                        value = binaryArith(typ, requireNum(a.value, typ), requireNum(b.value, typ));
                    }
                    return parent(typ, value, listOf(a, b));
                }
                case "nary": {
                    Object itemsRaw = node.get(ar.a);
                    if (!(itemsRaw instanceof List)) throw err(typ + " expects an '" + ar.a + "' list");
                    boolean isAnd = typ.equals(TX + "/And");
                    List<TraceNode> children = new ArrayList<>();
                    for (Object item : (List<Object>) itemsRaw) {
                        TraceNode child = trace((Map<String, Object>) item, ctx, resolve, options, frames);
                        children.add(child);
                        boolean v = truthy(requireNum(child.value, typ));
                        if (isAnd && !v) return parent(typ, 0.0, children);
                        if (!isAnd && v) return parent(typ, 1.0, children);
                    }
                    return parent(typ, bool(isAnd), children);
                }
                default: {
                    TraceNode tv = trace(operand(node, typ, ar.a), ctx, resolve, options, frames);
                    TraceNode tlo = trace(operand(node, typ, ar.b), ctx, resolve, options, frames);
                    TraceNode thi = trace(operand(node, typ, ar.c), ctx, resolve, options, frames);
                    double v = requireNum(tv.value, typ);
                    double lo = requireNum(tlo.value, typ);
                    double hi = requireNum(thi.value, typ);
                    return parent(typ, Math.min(Math.max(v, lo), hi), listOf(tv, tlo, thi));
                }
            }
        }

        if (typ.equals(TX + "/Not")) {
            TraceNode x = trace(operand(node, typ, "operand"), ctx, resolve, options, frames);
            return parent(typ, bool(!truthy(requireNum(x.value, typ))), listOf(x));
        }

        if (typ.equals(TX + "/IsAtLeast") || typ.equals(TX + "/Dominates")) {
            Object[] r = foldOrdered(node, typ, ctx, options);
            return new TraceNode(typ, r[0], new ArrayList<>(), (String) r[1], (String) r[2]);
        }

        if (isListFold(typ)) {
            TraceNode src = trace(operand(node, typ, "source"), ctx, resolve, options, frames);
            List<Object> items = src.value instanceof List ? (List<Object>) src.value
                : java.util.Collections.singletonList(src.value);
            return parent(typ, listFold(typ, items, node), listOf(src));
        }

        String bodyKey = iteratorBody(typ);
        if (bodyKey != null) {
            Object loopVarRaw = node.get("loopVar");
            if (!(loopVarRaw instanceof String) || ((String) loopVarRaw).isEmpty()) throw err(typ + " is missing loopVar");
            String loopVar = (String) loopVarRaw;
            TraceNode src = trace(operand(node, typ, "source"), ctx, resolve, options, frames);
            List<Object> items = src.value instanceof List ? (List<Object>) src.value
                : java.util.Collections.singletonList(src.value);
            Map<String, Object> body = operand(node, typ, bodyKey);
            List<TraceNode> children = new ArrayList<>();
            children.add(src);
            List<Object> out = new ArrayList<>();
            for (Object el : items) {
                frames.add(new Frame(loopVar, el));
                TraceNode bt;
                try {
                    bt = trace(body, ctx, resolve, options, frames);
                } finally {
                    frames.remove(frames.size() - 1);
                }
                children.add(bt);
                Object v = bt.value;
                if (typ.equals(TX + "/Filter")) {
                    if (truthy(requireNum(v, "Filter predicate"))) out.add(el);
                } else if (typ.equals(TX + "/ForEach")) {
                    if (v instanceof List) out.addAll((List<Object>) v);
                    else out.add(v);
                } else {
                    out.add(v);
                }
            }
            return parent(typ, out, children);
        }

        if (typ.equals(TX + "/Contains")) {
            TraceNode hay = trace(operand(node, typ, "haystack"), ctx, resolve, options, frames);
            TraceNode needle = trace(operand(node, typ, "needle"), ctx, resolve, options, frames);
            List<Object> items = hay.value instanceof List ? (List<Object>) hay.value
                : java.util.Collections.singletonList(hay.value);
            double v = 0.0;
            for (Object el : items) {
                if (valuesEqual(el, needle.value)) { v = 1.0; break; }
            }
            return parent(typ, v, listOf(hay, needle));
        }

        if (typ.equals(TX + "/IsSet")) {
            TraceNode x = trace(operand(node, typ, "checkExpr"), ctx, resolve, options, frames);
            return parent(typ, bool(isSet(x.value)), listOf(x));
        }

        if (typ.equals(TX + "/ListItemAt")) {
            TraceNode src = trace(operand(node, typ, "source"), ctx, resolve, options, frames);
            TraceNode idx = trace(operand(node, typ, "itemIndex"), ctx, resolve, options, frames);
            List<Object> items = src.value instanceof List ? (List<Object>) src.value
                : java.util.Collections.singletonList(src.value);
            Object iv = idx.value;
            if (!(iv instanceof Double) || (Double) iv != Math.floor((Double) iv) || (Double) iv < 0) {
                throw err("ListItemAt itemIndex must be a non-negative integer");
            }
            int i = (int) (double) (Double) iv;
            Object value = i < items.size() ? items.get(i) : new ArrayList<>();
            return parent(typ, value, listOf(src, idx));
        }

        if (typ.equals(TX + "/Matches")) {
            TraceNode src = trace(operand(node, typ, "matchSource"), ctx, resolve, options, frames);
            if (!(src.value instanceof String)) throw err("Matches requires a string matchSource, got " + kindOf(src.value));
            Object pattern = node.get("pattern");
            if (!(pattern instanceof String)) throw err("Matches is missing pattern");
            return parent(typ, bool(matchesPattern((String) src.value, (String) pattern)), listOf(src));
        }

        if (isKindPredicate(typ)) {
            TraceNode x = trace(operand(node, typ, "kindCheck"), ctx, resolve, options, frames);
            return parent(typ, bool(kindPredicate(typ, x.value)), listOf(x));
        }

        Object lit = literalValue(node, typ);
        if (lit != null) return leaf(typ, lit);

        if (typ.equals(TX + "/VarRef")) {
            Object name = node.get("varName");
            if (name instanceof String) {
                Object bound = boundValue(frames, (String) name);
                if (bound != null) return leaf(typ, bound);
            }
        }

        Object v = resolve.resolve(node, ctx, (n, c) -> go(n, c, resolve, options, new ArrayList<>()));
        return leaf(typ, v);
    }

    private static List<TraceNode> listOf(TraceNode... nodes) {
        List<TraceNode> out = new ArrayList<>(nodes.length);
        for (TraceNode n : nodes) out.add(n);
        return out;
    }
}
