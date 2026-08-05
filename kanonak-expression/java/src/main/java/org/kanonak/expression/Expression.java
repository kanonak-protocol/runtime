package org.kanonak.expression;

import java.util.List;
import java.util.Map;

/**
 * {@code kanonak-expression} — the Kanonak expression RUNTIME.
 *
 * <p>A small, deterministic tree-walker that folds a {@code kanonak.org/transformations}
 * ({@code tx}) + {@code kanonak.org/math} expression tree to a single number. Generated
 * SDKs reference it so a typed expression can be <em>run</em>, not just <em>represented</em>.
 *
 * <p>Three layers, exactly as the six-language proof established:
 *
 * <ol>
 *   <li>DISPATCH — derived from the ontology. An operator's arity falls out of its
 *       {@code tx} superclass: UnaryNumericOp -&gt; unary {@code value}; BinaryArithmetic /
 *       BinaryComparison -&gt; binary; BooleanLogic -&gt; n-ary {@code operands}; plus the two
 *       structural shapes the hierarchy can't imply ({@code Not}'s {@code operand},
 *       {@code Clip}'s ternary). The {@code OPERATOR_ARITY} table is that derivation, frozen.
 *   <li>PRIMITIVES — the one authored, determinism-bearing artifact. Each operator URI maps
 *       to its fold. Determinism traps live here and are matched in every language port
 *       (Round half-away-from-zero, floored Modulo, Sign(0)=0, comparisons as 1/0).
 *   <li>THE FOLD — {@link #evaluate}, a fixed shape: operators recurse + apply a primitive;
 *       literals return their numeric value; EVERYTHING ELSE (a typed VarRef, a domain
 *       {@code Step}/{@code Time}/{@code Smooth}, any future leaf) is handed to the caller's
 *       {@code resolve(node, ctx, evaluate)}. The runtime is a pure operator engine; binding
 *       and domain-leaf semantics are the caller's business. It never privileges
 *       {@code tx.VarRef} — that is just one leaf a domain may resolve.
 * </ol>
 *
 * <p>Value domain: uniform {@code double}. Booleans and comparison results are {@code 1}/{@code 0},
 * so every language stays on one numeric path. {@code EXPRESSION_RUNTIME_VERSION} freezes the
 * determinism contract; a change to any primitive, value rule, or dispatch requires a NEW
 * version, never an edit in place.
 */
public final class Expression {
    private Expression() {}

    /** The frozen expression-runtime version (determinism contract). Not hashed. */
    public static final String EXPRESSION_RUNTIME_VERSION = "1";

    private static final String TX = "kanonak.org/transformations";
    private static final String MATH = "kanonak.org/math";

    /**
     * Resolve any node the kernel does not recognise as an operator or literal — a binding
     * ({@code tx.VarRef}, a domain's typed {@code refersTo} VarRef) or a domain leaf
     * ({@code Step}, {@code Time}, {@code Smooth}…) — to a number. {@code ctx} is opaque caller
     * state (the binding env, a sim clock, integration state). {@code evaluate} is handed back
     * so a domain leaf containing sub-expressions can recurse into the kernel.
     *
     * @param <C> the opaque caller-context type
     */
    @FunctionalInterface
    public interface Resolve<C> {
        double resolve(Map<String, Object> node, C ctx, Recurse<C> evaluate);
    }

    /** The kernel's evaluate handed back to a {@link Resolve} so domain leaves can recurse. */
    @FunctionalInterface
    public interface Recurse<C> {
        double apply(Map<String, Object> node, C ctx);
    }

    /**
     * Resolve an identity leaf inside an ordered comparison — any operand node that is not a
     * {@code tx.UriLiteral} — to a member's canonical versionless URI
     * ({@code publisher/package/name}). The identity-domain mirror of {@link Resolve}: the
     * kernel owns the constant leaf, the caller owns bindings.
     *
     * @param <C> the opaque caller-context type
     */
    @FunctionalInterface
    public interface ResolveRef<C> {
        String resolve(Map<String, Object> node, C ctx);
    }

    /**
     * Optional evaluation context for the ordered comparisons ({@code IsAtLeast},
     * {@code Dominates}). {@code closures} is the transitive-closure data, keyed by the
     * ordering property's canonical URI, then by member: {@code closures[property][from]} is
     * the set of members {@code from} reaches — flat, already-closed data, typically the SDK
     * reasoner's {@code prp-trp} saturation emitted at code-generation time. The kernel does
     * set membership only; it never computes a closure, resolves a package, or reasons.
     * Absent (or missing a needed entry), an ordered comparison fails loudly — never a silent
     * false from a missing table.
     *
     * @param <C> the opaque caller-context type
     */
    public static final class EvalOptions<C> {
        final Map<String, Map<String, List<String>>> closures;
        final ResolveRef<C> resolveRef;
        public EvalOptions(Map<String, Map<String, List<String>>> closures, ResolveRef<C> resolveRef) {
            this.closures = closures;
            this.resolveRef = resolveRef;
        }
    }

    /**
     * One node of an evaluation trace — the verdict tree {@link #explain} returns. Mirrors the
     * expression: {@code type} is the node's type URI, {@code value} its result ({@code 1}/{@code 0}
     * for booleans), {@code children} the operand traces in evaluation order. A short-circuited
     * operand is simply ABSENT from {@code children} — the trace is truthful about what ran.
     * Ordered comparisons carry their resolved operand identities as {@code leftRef}/{@code rightRef}
     * (otherwise {@code null}) instead of children. A runtime return shape, not an ontology class.
     */
    public static final class TraceNode {
        public final String type;
        public final double value;
        public final List<TraceNode> children;
        public final String leftRef;
        public final String rightRef;
        TraceNode(String type, double value, List<TraceNode> children, String leftRef, String rightRef) {
            this.type = type;
            this.value = value;
            this.children = children;
            this.leftRef = leftRef;
            this.rightRef = rightRef;
        }
        TraceNode(String type, double value, List<TraceNode> children) {
            this(type, value, children, null, null);
        }
    }

    /** Raised on any determinism-contract violation (divide/modulo by zero, Ln/Log10 of ≤0, Sqrt of <0, malformed node). */
    public static final class ExpressionError extends RuntimeException {
        public ExpressionError(String message) { super(message); }
    }

    // -- Dispatch: operand shape per operator, derived from the tx superclass hierarchy. --

    private enum Kind { UNARY, BINARY, NARY, TERNARY }

    private static final class Arity {
        final Kind kind;
        final String a, b, c; // operand keys (semantics depend on kind)
        private Arity(Kind kind, String a, String b, String c) { this.kind = kind; this.a = a; this.b = b; this.c = c; }
        static Arity unary(String operand) { return new Arity(Kind.UNARY, operand, null, null); }
        static Arity binary(String left, String right) { return new Arity(Kind.BINARY, left, right, null); }
        static Arity nary(String operands) { return new Arity(Kind.NARY, operands, null, null); }
        static Arity ternary(String a, String b, String c) { return new Arity(Kind.TERNARY, a, b, c); }
    }

    private static final Arity ARITH = Arity.binary("arithLeft", "arithRight");
    private static final Arity COMPARE = Arity.binary("compareLeft", "compareRight");
    private static final Arity VALUE = Arity.unary("value");

    private static final Map<String, Arity> OPERATOR_ARITY = Map.ofEntries(
        Map.entry(TX + "/Add", ARITH),
        Map.entry(TX + "/Subtract", ARITH),
        Map.entry(TX + "/Multiply", ARITH),
        Map.entry(TX + "/Divide", ARITH),
        Map.entry(MATH + "/Power", ARITH),
        Map.entry(MATH + "/Modulo", ARITH),
        Map.entry(MATH + "/Minimum", ARITH),
        Map.entry(MATH + "/Maximum", ARITH),

        Map.entry(TX + "/Abs", VALUE),
        Map.entry(TX + "/Negate", VALUE),
        Map.entry(MATH + "/Exp", VALUE),
        Map.entry(MATH + "/Ln", VALUE),
        Map.entry(MATH + "/Log10", VALUE),
        Map.entry(MATH + "/Sqrt", VALUE),
        Map.entry(MATH + "/Floor", VALUE),
        Map.entry(MATH + "/Ceil", VALUE),
        Map.entry(MATH + "/Round", VALUE),
        Map.entry(MATH + "/Sign", VALUE),

        Map.entry(TX + "/Equals", COMPARE),
        Map.entry(TX + "/GreaterThan", COMPARE),
        Map.entry(TX + "/LessThan", COMPARE),
        Map.entry(TX + "/GreaterThanOrEqual", COMPARE),
        Map.entry(TX + "/LessThanOrEqual", COMPARE),

        Map.entry(TX + "/And", Arity.nary("operands")),
        Map.entry(TX + "/Or", Arity.nary("operands")),
        // `Not` is a direct Expression subclass with boolean (not numeric-unary)
        // semantics — handled explicitly in `evaluate`, not via the numeric tables.

        Map.entry(MATH + "/Clip", Arity.ternary("clipValue", "clipLower", "clipUpper"))
    );

    // -- Primitives: the authored, determinism-bearing folds, keyed by operator URI. --

    @FunctionalInterface private interface Unary { double apply(double x); }
    @FunctionalInterface private interface Binary { double apply(double a, double b); }

    private static final Map<String, Unary> UNARY = Map.ofEntries(
        Map.entry(TX + "/Abs", (Unary) Math::abs),
        Map.entry(TX + "/Negate", (Unary) x -> -x),
        Map.entry(MATH + "/Exp", (Unary) Math::exp),
        Map.entry(MATH + "/Ln", (Unary) x -> { requireDomain(x > 0, "Ln of a non-positive number"); return Math.log(x); }),
        Map.entry(MATH + "/Log10", (Unary) x -> { requireDomain(x > 0, "Log10 of a non-positive number"); return Math.log10(x); }),
        Map.entry(MATH + "/Sqrt", (Unary) x -> { requireDomain(x >= 0, "Sqrt of a negative number"); return Math.sqrt(x); }),
        Map.entry(MATH + "/Floor", (Unary) Math::floor),
        Map.entry(MATH + "/Ceil", (Unary) Math::ceil),
        Map.entry(MATH + "/Round", (Unary) Expression::roundHalfAway),
        Map.entry(MATH + "/Sign", (Unary) Math::signum)
    );

    private static final Map<String, Binary> BINARY = Map.ofEntries(
        Map.entry(TX + "/Add", (Binary) (a, b) -> a + b),
        Map.entry(TX + "/Subtract", (Binary) (a, b) -> a - b),
        Map.entry(TX + "/Multiply", (Binary) (a, b) -> a * b),
        Map.entry(TX + "/Divide", (Binary) (a, b) -> { requireDomain(b != 0, "Divide by zero"); return a / b; }),
        Map.entry(MATH + "/Power", (Binary) Math::pow),
        Map.entry(MATH + "/Modulo", (Binary) Expression::flooredMod),
        Map.entry(MATH + "/Minimum", (Binary) Math::min),
        Map.entry(MATH + "/Maximum", (Binary) Math::max),
        Map.entry(TX + "/Equals", (Binary) (a, b) -> bool(a == b)),
        Map.entry(TX + "/GreaterThan", (Binary) (a, b) -> bool(a > b)),
        Map.entry(TX + "/LessThan", (Binary) (a, b) -> bool(a < b)),
        Map.entry(TX + "/GreaterThanOrEqual", (Binary) (a, b) -> bool(a >= b)),
        Map.entry(TX + "/LessThanOrEqual", (Binary) (a, b) -> bool(a <= b))
    );

    /** Floored modulo (the host {@code %} truncates toward zero): Modulo(-7,3) = 2. */
    private static double flooredMod(double a, double b) {
        if (b == 0) throw new ExpressionError("Modulo by zero");
        return a - b * Math.floor(a / b);
    }

    /** Round half away from zero: Round(-2.5) = -3, Round(2.5) = 3. */
    private static double roundHalfAway(double a) {
        return a < 0 ? -Math.floor(-a + 0.5) : Math.floor(a + 0.5);
    }

    private static boolean truthy(double n) { return n != 0; }
    private static double bool(boolean b) { return b ? 1.0 : 0.0; }

    private static void requireDomain(boolean ok, String msg) {
        if (!ok) throw new ExpressionError(msg);
    }

    /**
     * Evaluate an expression tree to a number. Operators fold via the frozen dispatch +
     * primitive tables; literals yield their numeric value; any other node is delegated to
     * {@code resolve}.
     *
     * @param node    the expression node (a map with a {@code "type"} URI and operand keys)
     * @param ctx     opaque caller state
     * @param resolve the caller hook for bindings and domain leaves
     * @param <C>     the opaque caller-context type
     * @return the folded numeric value
     */
    public static <C> double evaluate(Map<String, Object> node, C ctx, Resolve<C> resolve) {
        return evaluate(node, ctx, resolve, null);
    }

    /**
     * {@link #evaluate(Map, Object, Resolve)} with the ordered-comparison evaluation context
     * (closures + identity-leaf resolution). {@code options} is only consulted when an
     * {@code IsAtLeast} / {@code Dominates} node is reached; {@code null} is valid for trees
     * without them.
     */
    public static <C> double evaluate(Map<String, Object> node, C ctx, Resolve<C> resolve, EvalOptions<C> options) {
        Recurse<C> recurse = (n, c) -> evaluate(n, c, resolve, options);
        String type = type(node);

        Arity arity = OPERATOR_ARITY.get(type);
        if (arity != null) {
            switch (arity.kind) {
                case UNARY: {
                    double x = recurse.apply(operand(node, arity.a), ctx);
                    return UNARY.get(type).apply(x);
                }
                case BINARY: {
                    double a = recurse.apply(operand(node, arity.a), ctx);
                    double b = recurse.apply(operand(node, arity.b), ctx);
                    return BINARY.get(type).apply(a, b);
                }
                case NARY: {
                    Object items = node.get(arity.a);
                    if (!(items instanceof List<?> list)) {
                        throw new ExpressionError(type + " expects an '" + arity.a + "' list");
                    }
                    boolean isAnd = (TX + "/And").equals(type);
                    // Short-circuit; empty And is vacuously true, empty Or vacuously false.
                    for (Object item : list) {
                        boolean v = truthy(recurse.apply(asNode(item), ctx));
                        if (isAnd && !v) return 0;
                        if (!isAnd && v) return 1;
                    }
                    return bool(isAnd);
                }
                case TERNARY: {
                    // Only Clip today: clamp clipValue into [clipLower, clipUpper].
                    double v = recurse.apply(operand(node, arity.a), ctx);
                    double lo = recurse.apply(operand(node, arity.b), ctx);
                    double hi = recurse.apply(operand(node, arity.c), ctx);
                    return Math.min(Math.max(v, lo), hi);
                }
            }
        }

        if ((TX + "/Not").equals(type)) {
            return bool(!truthy(recurse.apply(operand(node, "operand"), ctx)));
        }

        if ((TX + "/IsAtLeast").equals(type) || (TX + "/Dominates").equals(type)) {
            return foldOrdered(node, type, ctx, options).value;
        }

        Double lit = literalValue(node, type);
        if (lit != null) return lit;

        // Not an operator or literal — a binding or domain leaf. The caller owns it.
        return resolve.resolve(node, ctx, recurse);
    }

    /** The ordered fold's result: the verdict plus the resolved operand identities. */
    private static final class Ordered {
        final double value;
        final String left;
        final String right;
        Ordered(double value, String left, String right) { this.value = value; this.left = left; this.right = right; }
    }

    /**
     * The identity an ordered comparison compares — a member's canonical versionless URI.
     * {@code tx.UriLiteral} is the kernel-known constant leaf (its {@code refTo} IS the
     * identity, the way a literal's value is its number); every other node is the caller's,
     * through {@code options.resolveRef}.
     */
    private static <C> String identityOf(Map<String, Object> node, C ctx, EvalOptions<C> options) {
        if ((TX + "/UriLiteral").equals(type(node))) {
            Object ref = node.get("refTo");
            if (!(ref instanceof String s) || s.isEmpty()) {
                throw new ExpressionError("UriLiteral is missing refTo");
            }
            return s;
        }
        if (options == null || options.resolveRef == null) {
            throw new ExpressionError("No resolveRef supplied for identity leaf '" + type(node) + "'");
        }
        return options.resolveRef.resolve(node, ctx);
    }

    /**
     * Fold an ordered comparison ({@code IsAtLeast} / {@code Dominates}) to {@code 1}/{@code 0}
     * plus the resolved operand identities. The ordering is the supplied closure for the node's
     * {@code viaProperty} — membership in already-closed data, nothing more. Identity is
     * canonical versionless URI string equality, matching {@code tx.Equals}' identity rule.
     * {@code IsAtLeast} folds reflexivity into the operator (same member → 1); {@code Dominates}
     * is strict (same member → 0). Two members with no path yield 0 — fail-closed — but a
     * MISSING closure table is a configuration failure and errors loudly.
     */
    private static <C> Ordered foldOrdered(Map<String, Object> node, String type, C ctx, EvalOptions<C> options) {
        Object viaObj = node.get("viaProperty");
        if (!(viaObj instanceof String via) || via.isEmpty()) {
            throw new ExpressionError(type + " is missing viaProperty");
        }
        String left = identityOf(operand(node, "compareLeft"), ctx, options);
        String right = identityOf(operand(node, "compareRight"), ctx, options);
        Map<String, List<String>> closure = null;
        if (options != null && options.closures != null) closure = options.closures.get(via);
        if (closure == null) {
            throw new ExpressionError("No closure supplied for ordering property '" + via + "'");
        }
        double value;
        if (left.equals(right)) {
            value = bool((TX + "/IsAtLeast").equals(type));
        } else {
            List<String> reachable = closure.get(left);
            value = bool(reachable != null && reachable.contains(right));
        }
        return new Ordered(value, left, right);
    }

    /**
     * Evaluate an expression tree and return the verdict tree — the regex-debugger view: every
     * evaluated node, its own result, and (for ordered comparisons) the identities it compared.
     * The root's {@code value} is exactly what {@link #evaluate} returns for the same inputs;
     * the conformance suite runs every vector through both and requires agreement, so the two
     * entry points cannot drift. Kept separate from {@code evaluate} so the hot path never pays
     * for trace allocation. Errors propagate exactly as in {@code evaluate} — a failed
     * evaluation throws, never a partial trace.
     */
    public static <C> TraceNode explain(Map<String, Object> node, C ctx, Resolve<C> resolve, EvalOptions<C> options) {
        // Numeric recursion for subtrees the caller's resolve re-enters: those folds happen
        // inside the caller and are invisible to the trace. Only kernel-visited nodes appear.
        Recurse<C> recurseValue = (n, c) -> evaluate(n, c, resolve, options);
        String type = type(node);

        Arity arity = OPERATOR_ARITY.get(type);
        if (arity != null) {
            switch (arity.kind) {
                case UNARY: {
                    TraceNode x = explain(operand(node, arity.a), ctx, resolve, options);
                    return new TraceNode(type, UNARY.get(type).apply(x.value), List.of(x));
                }
                case BINARY: {
                    TraceNode a = explain(operand(node, arity.a), ctx, resolve, options);
                    TraceNode b = explain(operand(node, arity.b), ctx, resolve, options);
                    return new TraceNode(type, BINARY.get(type).apply(a.value, b.value), List.of(a, b));
                }
                case NARY: {
                    Object items = node.get(arity.a);
                    if (!(items instanceof List<?> list)) {
                        throw new ExpressionError(type + " expects an '" + arity.a + "' list");
                    }
                    boolean isAnd = (TX + "/And").equals(type);
                    java.util.ArrayList<TraceNode> children = new java.util.ArrayList<>();
                    for (Object item : list) {
                        TraceNode child = explain(asNode(item), ctx, resolve, options);
                        children.add(child);
                        boolean v = truthy(child.value);
                        // Same short-circuit as evaluate: operands after the deciding one are
                        // never evaluated and never appear in the trace.
                        if (isAnd && !v) return new TraceNode(type, 0, List.copyOf(children));
                        if (!isAnd && v) return new TraceNode(type, 1, List.copyOf(children));
                    }
                    return new TraceNode(type, bool(isAnd), List.copyOf(children));
                }
                case TERNARY: {
                    TraceNode v = explain(operand(node, arity.a), ctx, resolve, options);
                    TraceNode lo = explain(operand(node, arity.b), ctx, resolve, options);
                    TraceNode hi = explain(operand(node, arity.c), ctx, resolve, options);
                    double value = Math.min(Math.max(v.value, lo.value), hi.value);
                    return new TraceNode(type, value, List.of(v, lo, hi));
                }
            }
        }

        if ((TX + "/Not").equals(type)) {
            TraceNode x = explain(operand(node, "operand"), ctx, resolve, options);
            return new TraceNode(type, bool(!truthy(x.value)), List.of(x));
        }

        if ((TX + "/IsAtLeast").equals(type) || (TX + "/Dominates").equals(type)) {
            Ordered r = foldOrdered(node, type, ctx, options);
            return new TraceNode(type, r.value, List.of(), r.left, r.right);
        }

        Double lit = literalValue(node, type);
        if (lit != null) return new TraceNode(type, lit, List.of());

        return new TraceNode(type, resolve.resolve(node, ctx, recurseValue), List.of());
    }

    /** Numeric value of a literal node, or {@code null} if it is not a literal. */
    private static Double literalValue(Map<String, Object> node, String type) {
        switch (type) {
            case TX + "/IntegerLiteral": return number(node.get("integerLiteral"));
            case TX + "/DecimalLiteral": return number(node.get("decimalLiteral"));
            case TX + "/BooleanLiteral": {
                Object b = node.get("booleanLiteral");
                return bool(Boolean.TRUE.equals(b) || "true".equals(b));
            }
            default: return null;
        }
    }

    private static double number(Object v) {
        if (v instanceof Number n) return n.doubleValue();
        if (v instanceof String s) return Double.parseDouble(s);
        throw new ExpressionError("expected a numeric literal, got " + v);
    }

    private static String type(Map<String, Object> node) {
        Object t = node.get("type");
        if (!(t instanceof String s)) throw new ExpressionError("node is missing a string 'type'");
        return s;
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> asNode(Object v) {
        if (!(v instanceof Map<?, ?> m)) throw new ExpressionError("expected an expression node, got " + v);
        return (Map<String, Object>) m;
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> operand(Map<String, Object> node, String key) {
        Object v = node.get(key);
        if (!(v instanceof Map<?, ?> m)) {
            throw new ExpressionError(type(node) + " is missing operand '" + key + "'");
        }
        return (Map<String, Object>) m;
    }
}
