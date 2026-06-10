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
        Recurse<C> recurse = (n, c) -> evaluate(n, c, resolve);
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

        Double lit = literalValue(node, type);
        if (lit != null) return lit;

        // Not an operator or literal — a binding or domain leaf. The caller owns it.
        return resolve.resolve(node, ctx, recurse);
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
