using System;
using System.Collections.Generic;

namespace Kanonak.Expression
{
    /// <summary>
    /// A node in the expression tree. <see cref="Type"/> is the operator/literal/leaf
    /// canonical URI (versionless: <c>publisher/package/name</c>); operand keys are the
    /// frozen <c>tx</c> operand property local names, held in <see cref="Fields"/>.
    /// Unknown fields are ignored by the kernel and available to <c>resolve</c> for
    /// domain leaves.
    /// </summary>
    public sealed class ExprNode
    {
        public string Type;
        public Dictionary<string, object> Fields;

        public ExprNode(string type, Dictionary<string, object> fields = null)
        {
            Type = type;
            Fields = fields ?? new Dictionary<string, object>();
        }

        public object Get(string key)
        {
            return Fields != null && Fields.TryGetValue(key, out var v) ? v : null;
        }
    }

    /// <summary>An error raised by the expression runtime (domain/value violation).</summary>
    public sealed class ExpressionError : Exception
    {
        public ExpressionError(string message) : base(message) { }
    }

    /// <summary>
    /// Resolve any node the kernel does not recognise as an operator or literal — a
    /// binding (<c>tx.VarRef</c>, a domain's typed <c>refersTo</c> VarRef) or a domain
    /// leaf (<c>Step</c>, <c>Time</c>, <c>Smooth</c>…) — to a number. <c>ctx</c> is
    /// opaque caller state; <c>evaluate</c> is handed back so a domain leaf containing
    /// sub-expressions can recurse into the kernel.
    /// </summary>
    public delegate double Resolve(ExprNode node, object ctx, Func<ExprNode, object, double> evaluate);

    /// <summary>
    /// Resolve an identity leaf inside an ordered comparison — any operand node that is
    /// not a <c>tx.UriLiteral</c> — to a member's canonical versionless URI
    /// (<c>publisher/package/name</c>). The identity-domain mirror of <see cref="Resolve"/>:
    /// the kernel owns the constant leaf, the caller owns bindings.
    /// </summary>
    public delegate string ResolveRef(ExprNode node, object ctx);

    /// <summary>
    /// Optional evaluation context for the ordered comparisons (<c>IsAtLeast</c>,
    /// <c>Dominates</c>). <see cref="Closures"/> is the transitive-closure data, keyed by
    /// the ordering property's canonical URI, then by member:
    /// <c>Closures[property][from]</c> is the set of members <c>from</c> reaches — flat,
    /// already-closed data, typically the SDK reasoner's <c>prp-trp</c> saturation emitted
    /// at code-generation time. The kernel does set membership only; it never computes a
    /// closure, resolves a package, or reasons. Absent (or missing a needed entry), an
    /// ordered comparison fails loudly — never a silent false from a missing table.
    /// </summary>
    public sealed class EvalOptions
    {
        public Dictionary<string, Dictionary<string, List<string>>> Closures;
        public ResolveRef ResolveRef;
    }

    /// <summary>
    /// One node of an evaluation trace — the verdict tree <c>Expr.Explain</c> returns.
    /// Mirrors the expression: <see cref="Type"/> is the node's type URI, <see cref="Value"/>
    /// its result (1/0 for booleans), <see cref="Children"/> the operand traces in evaluation
    /// order. A short-circuited operand is simply ABSENT from Children — the trace is truthful
    /// about what ran. Ordered comparisons carry their resolved operand identities as
    /// <see cref="LeftRef"/>/<see cref="RightRef"/> (otherwise null) instead of children.
    /// A runtime return shape, not an ontology class.
    /// </summary>
    public sealed class TraceNode
    {
        public string Type;
        public double Value;
        public List<TraceNode> Children = new List<TraceNode>();
        public string LeftRef;
        public string RightRef;
    }

    /// <summary>
    /// The Kanonak expression runtime: a small, deterministic tree-walker that folds a
    /// <c>kanonak.org/transformations</c> (<c>tx</c>) + <c>kanonak.org/math</c> expression
    /// tree to a single number. Operators fold via the frozen dispatch + primitive tables;
    /// literals yield their numeric value; everything else is delegated to <c>resolve</c>.
    ///
    /// <see cref="ExpressionRuntimeVersion"/> freezes the determinism contract; a change to
    /// any primitive, value rule, or dispatch entry requires a NEW version, never an edit
    /// in place.
    /// </summary>
    public static class Expr
    {
        /// <summary>The frozen expression-runtime version (determinism contract). Not hashed.</summary>
        public const string ExpressionRuntimeVersion = "1";

        private const string TX = "kanonak.org/transformations";
        private const string MATH = "kanonak.org/math";

        // -- Dispatch — operand shape per operator, derived from the tx superclass hierarchy.

        private enum Kind { Unary, Binary, Nary, Ternary }

        private struct Arity
        {
            public Kind Kind;
            public string A;
            public string B;
            public string C;
        }

        private static Arity Un(string operand) => new Arity { Kind = Kind.Unary, A = operand };
        private static Arity Bin(string left, string right) => new Arity { Kind = Kind.Binary, A = left, B = right };

        private static readonly Arity ARITH = Bin("arithLeft", "arithRight");
        private static readonly Arity COMPARE = Bin("compareLeft", "compareRight");
        private static readonly Arity VALUE = Un("value");

        private static readonly Dictionary<string, Arity> OperatorArity = BuildArity();

        private static Dictionary<string, Arity> BuildArity()
        {
            var m = new Dictionary<string, Arity>
            {
                [$"{TX}/Add"] = ARITH,
                [$"{TX}/Subtract"] = ARITH,
                [$"{TX}/Multiply"] = ARITH,
                [$"{TX}/Divide"] = ARITH,
                [$"{MATH}/Power"] = ARITH,
                [$"{MATH}/Modulo"] = ARITH,
                [$"{MATH}/Minimum"] = ARITH,
                [$"{MATH}/Maximum"] = ARITH,

                [$"{TX}/Abs"] = VALUE,
                [$"{TX}/Negate"] = VALUE,
                [$"{MATH}/Exp"] = VALUE,
                [$"{MATH}/Ln"] = VALUE,
                [$"{MATH}/Log10"] = VALUE,
                [$"{MATH}/Sqrt"] = VALUE,
                [$"{MATH}/Floor"] = VALUE,
                [$"{MATH}/Ceil"] = VALUE,
                [$"{MATH}/Round"] = VALUE,
                [$"{MATH}/Sign"] = VALUE,

                [$"{TX}/Equals"] = COMPARE,
                [$"{TX}/GreaterThan"] = COMPARE,
                [$"{TX}/LessThan"] = COMPARE,
                [$"{TX}/GreaterThanOrEqual"] = COMPARE,
                [$"{TX}/LessThanOrEqual"] = COMPARE,

                [$"{TX}/And"] = new Arity { Kind = Kind.Nary, A = "operands" },
                [$"{TX}/Or"] = new Arity { Kind = Kind.Nary, A = "operands" },
                // `Not` is a direct Expression subclass with boolean (not numeric-unary)
                // semantics — handled explicitly in Evaluate, not via the numeric tables.

                [$"{MATH}/Clip"] = new Arity { Kind = Kind.Ternary, A = "clipValue", B = "clipLower", C = "clipUpper" },
            };
            return m;
        }

        // -- Primitives — the authored, determinism-bearing tables, matched per language.

        private static readonly Dictionary<string, Func<double, double>> Unary = BuildUnary();
        private static readonly Dictionary<string, Func<double, double, double>> Binary = BuildBinary();

        private static Dictionary<string, Func<double, double>> BuildUnary()
        {
            return new Dictionary<string, Func<double, double>>
            {
                [$"{TX}/Abs"] = x => Math.Abs(x),
                [$"{TX}/Negate"] = x => -x,
                [$"{MATH}/Exp"] = x => Math.Exp(x),
                [$"{MATH}/Ln"] = x => { RequireDomain(x > 0, "Ln of a non-positive number"); return Math.Log(x); },
                [$"{MATH}/Log10"] = x => { RequireDomain(x > 0, "Log10 of a non-positive number"); return Math.Log10(x); },
                [$"{MATH}/Sqrt"] = x => { RequireDomain(x >= 0, "Sqrt of a negative number"); return Math.Sqrt(x); },
                [$"{MATH}/Floor"] = x => Math.Floor(x),
                [$"{MATH}/Ceil"] = x => Math.Ceiling(x),
                [$"{MATH}/Round"] = RoundHalfAway,
                [$"{MATH}/Sign"] = x => Math.Sign(x),
            };
        }

        private static Dictionary<string, Func<double, double, double>> BuildBinary()
        {
            return new Dictionary<string, Func<double, double, double>>
            {
                [$"{TX}/Add"] = (a, b) => a + b,
                [$"{TX}/Subtract"] = (a, b) => a - b,
                [$"{TX}/Multiply"] = (a, b) => a * b,
                [$"{TX}/Divide"] = (a, b) => { RequireDomain(b != 0, "Divide by zero"); return a / b; },
                [$"{MATH}/Power"] = (a, b) => Math.Pow(a, b),
                [$"{MATH}/Modulo"] = FlooredMod,
                [$"{MATH}/Minimum"] = (a, b) => Math.Min(a, b),
                [$"{MATH}/Maximum"] = (a, b) => Math.Max(a, b),
                [$"{TX}/Equals"] = (a, b) => Bool(a == b),
                [$"{TX}/GreaterThan"] = (a, b) => Bool(a > b),
                [$"{TX}/LessThan"] = (a, b) => Bool(a < b),
                [$"{TX}/GreaterThanOrEqual"] = (a, b) => Bool(a >= b),
                [$"{TX}/LessThanOrEqual"] = (a, b) => Bool(a <= b),
            };
        }

        /// <summary>Floored modulo (the host operator truncates toward zero): Modulo(-7,3) = 2.</summary>
        private static double FlooredMod(double a, double b)
        {
            if (b == 0) throw new ExpressionError("Modulo by zero");
            return a - b * Math.Floor(a / b);
        }

        /// <summary>Round half away from zero: Round(-2.5) = -3, Round(2.5) = 3.</summary>
        private static double RoundHalfAway(double a)
        {
            return a < 0 ? -Math.Floor(-a + 0.5) : Math.Floor(a + 0.5);
        }

        private static bool Truthy(double n) => n != 0;
        private static double Bool(bool b) => b ? 1.0 : 0.0;

        private static void RequireDomain(bool ok, string msg)
        {
            if (!ok) throw new ExpressionError(msg);
        }

        /// <summary>Numeric value of a literal node, or null if it is not a literal.</summary>
        private static double? LiteralValue(ExprNode node)
        {
            switch (node.Type)
            {
                case TX + "/IntegerLiteral": return ToNumber(node.Get("integerLiteral"));
                case TX + "/DecimalLiteral": return ToNumber(node.Get("decimalLiteral"));
                case TX + "/BooleanLiteral":
                    {
                        var raw = node.Get("booleanLiteral");
                        bool t = (raw is bool b && b) || (raw is string s && s == "true");
                        return Bool(t);
                    }
                default: return null;
            }
        }

        private static double ToNumber(object o)
        {
            switch (o)
            {
                case null: throw new ExpressionError("literal is missing its value");
                case double d: return d;
                case float f: return f;
                case int i: return i;
                case long l: return l;
                case decimal m: return (double)m;
                case bool b: return b ? 1.0 : 0.0;
                case string s: return double.Parse(s, System.Globalization.CultureInfo.InvariantCulture);
                default: return Convert.ToDouble(o, System.Globalization.CultureInfo.InvariantCulture);
            }
        }

        /// <summary>
        /// Evaluate an expression tree to a number. Operators fold via the frozen dispatch +
        /// primitive tables; literals yield their numeric value; any other node is delegated
        /// to <paramref name="resolve"/>.
        /// </summary>
        public static double Evaluate(ExprNode node, object ctx, Resolve resolve)
        {
            return Evaluate(node, ctx, resolve, null);
        }

        /// <summary>
        /// <see cref="Evaluate(ExprNode, object, Resolve)"/> with the ordered-comparison
        /// evaluation context (closures + identity-leaf resolution). <paramref name="options"/>
        /// is only consulted when an <c>IsAtLeast</c> / <c>Dominates</c> node is reached; null
        /// is valid for trees without them.
        /// </summary>
        public static double Evaluate(ExprNode node, object ctx, Resolve resolve, EvalOptions options)
        {
            Func<ExprNode, object, double> recurse = null;
            recurse = (n, c) => Evaluate(n, c, resolve, options);

            if (OperatorArity.TryGetValue(node.Type, out var arity))
            {
                switch (arity.Kind)
                {
                    case Kind.Unary:
                        {
                            double x = recurse(Operand(node, arity.A), ctx);
                            return Unary[node.Type](x);
                        }
                    case Kind.Binary:
                        {
                            double a = recurse(Operand(node, arity.A), ctx);
                            double b = recurse(Operand(node, arity.B), ctx);
                            return Binary[node.Type](a, b);
                        }
                    case Kind.Nary:
                        {
                            var items = node.Get(arity.A) as System.Collections.IEnumerable;
                            if (items == null || node.Get(arity.A) is string)
                                throw new ExpressionError($"{node.Type} expects an '{arity.A}' list");
                            bool isAnd = node.Type == $"{TX}/And";
                            // Short-circuit; empty And is vacuously true, empty Or vacuously false.
                            foreach (var item in items)
                            {
                                bool v = Truthy(recurse(AsNode(item), ctx));
                                if (isAnd && !v) return 0;
                                if (!isAnd && v) return 1;
                            }
                            return Bool(isAnd);
                        }
                    case Kind.Ternary:
                        {
                            // Only Clip today: clamp clipValue into [clipLower, clipUpper].
                            double v = recurse(Operand(node, arity.A), ctx);
                            double lo = recurse(Operand(node, arity.B), ctx);
                            double hi = recurse(Operand(node, arity.C), ctx);
                            return Math.Min(Math.Max(v, lo), hi);
                        }
                }
            }

            if (node.Type == $"{TX}/Not")
            {
                return Bool(!Truthy(recurse(Operand(node, "operand"), ctx)));
            }

            if (node.Type == $"{TX}/IsAtLeast" || node.Type == $"{TX}/Dominates")
            {
                return FoldOrdered(node, ctx, options).Value;
            }

            double? lit = LiteralValue(node);
            if (lit.HasValue) return lit.Value;

            // Not an operator or literal — a binding or domain leaf. The caller owns it.
            return resolve(node, ctx, recurse);
        }

        /// <summary>The ordered fold's result: the verdict plus the resolved operand identities.</summary>
        private struct Ordered
        {
            public double Value;
            public string Left;
            public string Right;
        }

        /// <summary>
        /// The identity an ordered comparison compares — a member's canonical versionless URI.
        /// <c>tx.UriLiteral</c> is the kernel-known constant leaf (its <c>refTo</c> IS the
        /// identity, the way a literal's value is its number); every other node is the
        /// caller's, through <c>options.ResolveRef</c>.
        /// </summary>
        private static string IdentityOf(ExprNode node, object ctx, EvalOptions options)
        {
            if (node.Type == $"{TX}/UriLiteral")
            {
                if (!(node.Get("refTo") is string s) || s.Length == 0)
                    throw new ExpressionError("UriLiteral is missing refTo");
                return s;
            }
            if (options?.ResolveRef == null)
                throw new ExpressionError($"No resolveRef supplied for identity leaf '{node.Type}'");
            return options.ResolveRef(node, ctx);
        }

        /// <summary>
        /// Fold an ordered comparison (<c>IsAtLeast</c> / <c>Dominates</c>) to 1/0 plus the
        /// resolved operand identities. The ordering is the supplied closure for the node's
        /// <c>viaProperty</c> — membership in already-closed data, nothing more. Identity is
        /// canonical versionless URI string equality, matching <c>tx.Equals</c>' identity
        /// rule. <c>IsAtLeast</c> folds reflexivity into the operator (same member → 1);
        /// <c>Dominates</c> is strict (same member → 0). Two members with no path yield 0 —
        /// fail-closed — but a MISSING closure table is a configuration failure and errors
        /// loudly.
        /// </summary>
        private static Ordered FoldOrdered(ExprNode node, object ctx, EvalOptions options)
        {
            if (!(node.Get("viaProperty") is string via) || via.Length == 0)
                throw new ExpressionError($"{node.Type} is missing viaProperty");
            string left = IdentityOf(Operand(node, "compareLeft"), ctx, options);
            string right = IdentityOf(Operand(node, "compareRight"), ctx, options);
            Dictionary<string, List<string>> closure = null;
            if (options?.Closures != null) options.Closures.TryGetValue(via, out closure);
            if (closure == null)
                throw new ExpressionError($"No closure supplied for ordering property '{via}'");
            double value;
            if (left == right)
            {
                value = Bool(node.Type == $"{TX}/IsAtLeast");
            }
            else
            {
                value = Bool(closure.TryGetValue(left, out var reachable) && reachable.Contains(right));
            }
            return new Ordered { Value = value, Left = left, Right = right };
        }

        /// <summary>
        /// Evaluate an expression tree and return the verdict tree — the regex-debugger view:
        /// every evaluated node, its own result, and (for ordered comparisons) the identities
        /// it compared. The root's <c>Value</c> is exactly what <c>Evaluate</c> returns for
        /// the same inputs; the conformance suite runs every vector through both and requires
        /// agreement, so the two entry points cannot drift. Kept separate from Evaluate so the
        /// hot path never pays for trace allocation. Errors propagate exactly as in Evaluate —
        /// a failed evaluation throws, never a partial trace.
        /// </summary>
        public static TraceNode Explain(ExprNode node, object ctx, Resolve resolve, EvalOptions options)
        {
            // Numeric recursion for subtrees the caller's resolve re-enters: those folds
            // happen inside the caller and are invisible to the trace. Only kernel-visited
            // nodes appear.
            Func<ExprNode, object, double> recurseValue = (n, c) => Evaluate(n, c, resolve, options);

            if (OperatorArity.TryGetValue(node.Type, out var arity))
            {
                switch (arity.Kind)
                {
                    case Kind.Unary:
                        {
                            var x = Explain(Operand(node, arity.A), ctx, resolve, options);
                            return new TraceNode { Type = node.Type, Value = Unary[node.Type](x.Value), Children = { x } };
                        }
                    case Kind.Binary:
                        {
                            var a = Explain(Operand(node, arity.A), ctx, resolve, options);
                            var b = Explain(Operand(node, arity.B), ctx, resolve, options);
                            return new TraceNode { Type = node.Type, Value = Binary[node.Type](a.Value, b.Value), Children = { a, b } };
                        }
                    case Kind.Nary:
                        {
                            var items = node.Get(arity.A) as System.Collections.IEnumerable;
                            if (items == null || node.Get(arity.A) is string)
                                throw new ExpressionError($"{node.Type} expects an '{arity.A}' list");
                            bool isAnd = node.Type == $"{TX}/And";
                            var children = new List<TraceNode>();
                            foreach (var item in items)
                            {
                                var child = Explain(AsNode(item), ctx, resolve, options);
                                children.Add(child);
                                bool v = Truthy(child.Value);
                                // Same short-circuit as Evaluate: operands after the deciding
                                // one are never evaluated and never appear in the trace.
                                if (isAnd && !v) return new TraceNode { Type = node.Type, Value = 0, Children = children };
                                if (!isAnd && v) return new TraceNode { Type = node.Type, Value = 1, Children = children };
                            }
                            return new TraceNode { Type = node.Type, Value = Bool(isAnd), Children = children };
                        }
                    case Kind.Ternary:
                        {
                            var v = Explain(Operand(node, arity.A), ctx, resolve, options);
                            var lo = Explain(Operand(node, arity.B), ctx, resolve, options);
                            var hi = Explain(Operand(node, arity.C), ctx, resolve, options);
                            double value = Math.Min(Math.Max(v.Value, lo.Value), hi.Value);
                            return new TraceNode { Type = node.Type, Value = value, Children = { v, lo, hi } };
                        }
                }
            }

            if (node.Type == $"{TX}/Not")
            {
                var x = Explain(Operand(node, "operand"), ctx, resolve, options);
                return new TraceNode { Type = node.Type, Value = Bool(!Truthy(x.Value)), Children = { x } };
            }

            if (node.Type == $"{TX}/IsAtLeast" || node.Type == $"{TX}/Dominates")
            {
                var r = FoldOrdered(node, ctx, options);
                return new TraceNode { Type = node.Type, Value = r.Value, LeftRef = r.Left, RightRef = r.Right };
            }

            double? lit = LiteralValue(node);
            if (lit.HasValue) return new TraceNode { Type = node.Type, Value = lit.Value };

            return new TraceNode { Type = node.Type, Value = resolve(node, ctx, recurseValue) };
        }

        private static ExprNode Operand(ExprNode node, string key)
        {
            object v = node.Get(key);
            if (v == null) throw new ExpressionError($"{node.Type} is missing operand '{key}'");
            return AsNode(v);
        }

        private static ExprNode AsNode(object v)
        {
            if (v is ExprNode n) return n;
            throw new ExpressionError("operand is not an expression node");
        }
    }
}
