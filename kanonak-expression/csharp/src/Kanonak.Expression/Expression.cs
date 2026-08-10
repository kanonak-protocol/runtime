using System;
using System.Collections.Generic;
using System.Text;
using System.Text.RegularExpressions;

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

    /// <summary>A reference value — a member's canonical versionless URI identity.
    /// Distinct from string so Equals holds the cross-kind-is-false rule.</summary>
    public sealed class Ref : IEquatable<Ref>
    {
        public readonly string Uri;
        public Ref(string uri) { Uri = uri; }
        public bool Equals(Ref other) => other != null && other.Uri == Uri;
        public override bool Equals(object o) => o is Ref r && Equals(r);
        public override int GetHashCode() => Uri.GetHashCode();
        public override string ToString() => "Ref(" + Uri + ")";
    }

    /// <summary>An error raised by the expression runtime (domain/value violation).</summary>
    public sealed class ExpressionError : Exception
    {
        public ExpressionError(string message) : base(message) { }
    }

    /// <summary>
    /// Resolve any node the kernel does not recognise as an operator or literal — a
    /// binding (<c>tx.VarRef</c>), a host graph read (a property-read leaf returning a
    /// list), or a domain leaf — to a value (<c>double | string | Ref | List&lt;object&gt;</c>).
    /// <c>ctx</c> is opaque caller state; <c>evaluate</c> is handed back so a domain leaf
    /// containing sub-expressions can recurse into the kernel (WITHOUT lambda frames —
    /// the caller's subtrees are the caller's scope).
    /// </summary>
    public delegate object Resolve(ExprNode node, object ctx, Func<ExprNode, object, object> evaluate);

    /// <summary>Resolve an identity leaf inside an ordered comparison.</summary>
    public delegate string ResolveRef(ExprNode node, object ctx);

    /// <summary>Optional evaluation context for the ordered comparisons.</summary>
    public sealed class EvalOptions
    {
        public Dictionary<string, Dictionary<string, List<string>>> Closures;
        public ResolveRef ResolveRef;
    }

    /// <summary>
    /// One node of an evaluation trace — the verdict tree <c>Expr.Explain</c> returns.
    /// Short-circuited operands are ABSENT from Children; an iterating operator's children
    /// are its source trace followed by one body trace per visited element. A runtime
    /// return shape, not an ontology class.
    /// </summary>
    public sealed class TraceNode
    {
        public string Type;
        public object Value;
        public List<TraceNode> Children = new List<TraceNode>();
        public string LeftRef;
        public string RightRef;
    }

    /// <summary>
    /// Kanonak expression runtime (expressionRuntimeVersion "2") — a small, deterministic
    /// tree-walker that folds a <c>kanonak.org/transformations</c> + <c>kanonak.org/math</c>
    /// expression tree to a VALUE. An independent conformant C# port of the reference
    /// kernel, verified against the shared parity vectors.
    ///
    /// <para>VALUE DOMAIN (v2): <c>double | string | Ref | List&lt;object&gt;</c>. Booleans
    /// and comparison results remain 1/0; absence is the empty list — there is no null
    /// value. The kernel NEVER touches a graph.</para>
    ///
    /// <para>ERROR CONTRACT: computations fail LOUD; predicates fail CLOSED (Equals
    /// cross-kind and the ordering comparisons on non-numbers yield 0).</para>
    ///
    /// <para>MATCHES: the pinned RE2-compatible XSD-regex subset. .NET's Regex counts
    /// UTF-16 CODE UNITS — the pinned unit is CODE POINTS — so this engine owes the
    /// largest translations: <c>.</c> compiles to a surrogate-pair-aware alternation
    /// (one astral character is one <c>.</c>), and <c>\b</c>/<c>\B</c> compile to
    /// explicit ASCII-word-class lookarounds (.NET's native <c>\b</c> is Unicode).
    /// The shorthand expansions are load-bearing (.NET's native <c>\d</c>/<c>\w</c>/<c>\s</c>
    /// are Unicode); case folding pins to Unicode simple folding via
    /// IgnoreCase|CultureInvariant. Host lookaround in the COMPILED form is fine — the
    /// subset restriction is on what authors write.</para>
    /// </summary>
    public static class Expr
    {
        /// <summary>The frozen expression-runtime version (determinism contract). Not hashed.</summary>
        public const string ExpressionRuntimeVersion = "2";

        const string TX = "kanonak.org/transformations";
        const string MATH = "kanonak.org/math";

        static ExpressionError Err(string msg) => new ExpressionError(msg);

        // -- helpers ----------------------------------------------------------

        static string KindOf(object v)
        {
            if (v is double) return "number";
            if (v is string) return "string";
            if (v is Ref) return "ref";
            if (v is List<object>) return "list";
            return "unknown";
        }

        static double RequireNum(object v, string op)
        {
            if (v is double d) return d;
            throw Err(op + " requires a numeric operand, got " + KindOf(v));
        }

        static bool Truthy(double n) => n != 0.0;
        static double Bool(bool b) => b ? 1.0 : 0.0;

        static void RequireDomain(bool ok, string msg) { if (!ok) throw Err(msg); }

        static double FlooredMod(double a, double b)
        {
            RequireDomain(b != 0.0, "Modulo by zero");
            return a - b * Math.Floor(a / b);
        }

        static double RoundHalfAway(double a)
            => a < 0 ? -Math.Floor(-a + 0.5) : Math.Floor(a + 0.5);

        /// <summary>Polymorphic tx.Equals: scalars by value, refs by URI identity, lists
        /// never equal, cross-kind false. Never errors.</summary>
        static bool ValuesEqual(object a, object b)
        {
            if (a is double x && b is double y) return x == y;
            if (a is string s && b is string t) return s == t;
            if (a is Ref r && b is Ref q) return r.Equals(q);
            return false;
        }

        static ExprNode Operand(ExprNode node, string key)
        {
            if (node.Get(key) is ExprNode n) return n;
            throw Err(node.Type + " is missing operand '" + key + "'");
        }

        static double ToNumber(object v)
        {
            if (v is double d) return d;
            if (v is int i) return i;
            if (v is long l) return l;
            if (v is string s && double.TryParse(s, System.Globalization.NumberStyles.Float,
                System.Globalization.CultureInfo.InvariantCulture, out var p)) return p;
            throw Err("not a number: " + v);
        }

        /// <summary>A literal node's value, or null when not a literal (the domain has no
        /// null values, so null is safe as the not-a-literal sentinel).</summary>
        static object LiteralValue(ExprNode node)
        {
            switch (node.Type)
            {
                case TX + "/IntegerLiteral": return ToNumber(node.Get("integerLiteral"));
                case TX + "/DecimalLiteral": return ToNumber(node.Get("decimalLiteral"));
                case TX + "/BooleanLiteral":
                {
                    var b = node.Get("booleanLiteral");
                    return Bool(true.Equals(b) || "true".Equals(b));
                }
                case TX + "/StringLiteral":
                {
                    if (node.Get("stringLiteral") is string s) return s;
                    throw Err("StringLiteral is missing stringLiteral");
                }
                case TX + "/UriLiteral":
                {
                    if (node.Get("refTo") is string r && r.Length > 0) return new Ref(r);
                    throw Err("UriLiteral is missing refTo");
                }
                default: return null;
            }
        }

        /// <summary>ECMAScript-style number-to-string (the RFC 8785 rule): integral
        /// without a decimal point; else shortest round-trip.</summary>
        static string FormatNumber(double n)
        {
            if (n == Math.Floor(n) && Math.Abs(n) < 1e21)
                return ((long)n).ToString(System.Globalization.CultureInfo.InvariantCulture);
            return n.ToString("R", System.Globalization.CultureInfo.InvariantCulture);
        }

        static string JoinElement(object v)
        {
            if (v is string s) return s;
            if (v is double d) return FormatNumber(d);
            if (v is Ref r)
            {
                int i = r.Uri.LastIndexOf('/');
                return i >= 0 ? r.Uri.Substring(i + 1) : r.Uri;
            }
            throw Err("Join cannot stringify a nested list");
        }

        static bool IsSetValue(object v)
        {
            if (v is string s) return s.Length > 0;
            if (v is List<object> l) return l.Count > 0;
            return true;
        }

        // -- dispatch ---------------------------------------------------------

        struct Arity
        {
            public string Kind, A, B, C;
            public Arity(string kind, string a, string b = null, string c = null)
            { Kind = kind; A = a; B = b; C = c; }
        }

        static readonly Arity Arith = new Arity("binary", "arithLeft", "arithRight");
        static readonly Arity Compare = new Arity("binary", "compareLeft", "compareRight");
        static readonly Arity ValueA = new Arity("unary", "value");

        static Arity? OperatorArity(string typ)
        {
            switch (typ)
            {
                case TX + "/Add": case TX + "/Subtract": case TX + "/Multiply": case TX + "/Divide":
                case MATH + "/Power": case MATH + "/Modulo": case MATH + "/Minimum": case MATH + "/Maximum":
                    return Arith;
                case TX + "/Abs": case TX + "/Negate": case MATH + "/Exp": case MATH + "/Ln":
                case MATH + "/Log10": case MATH + "/Sqrt": case MATH + "/Floor": case MATH + "/Ceil":
                case MATH + "/Round": case MATH + "/Sign":
                    return ValueA;
                case TX + "/Equals": case TX + "/GreaterThan": case TX + "/LessThan":
                case TX + "/GreaterThanOrEqual": case TX + "/LessThanOrEqual":
                    return Compare;
                case TX + "/And": case TX + "/Or":
                    return new Arity("nary", "operands");
                case MATH + "/Clip":
                    return new Arity("ternary", "clipValue", "clipLower", "clipUpper");
                default:
                    return null;
            }
        }

        static double Unary(string typ, double x)
        {
            switch (typ)
            {
                case TX + "/Abs": return Math.Abs(x);
                case TX + "/Negate": return -x;
                case MATH + "/Exp": return Math.Exp(x);
                case MATH + "/Ln": RequireDomain(x > 0, "Ln of a non-positive number"); return Math.Log(x);
                case MATH + "/Log10": RequireDomain(x > 0, "Log10 of a non-positive number"); return Math.Log10(x);
                case MATH + "/Sqrt": RequireDomain(x >= 0, "Sqrt of a negative number"); return Math.Sqrt(x);
                case MATH + "/Floor": return Math.Floor(x);
                case MATH + "/Ceil": return Math.Ceiling(x);
                case MATH + "/Round": return RoundHalfAway(x);
                case MATH + "/Sign": return Math.Sign(x);
                default: throw Err("No unary primitive for " + typ);
            }
        }

        static double BinaryArith(string typ, double a, double b)
        {
            switch (typ)
            {
                case TX + "/Add": return a + b;
                case TX + "/Subtract": return a - b;
                case TX + "/Multiply": return a * b;
                case TX + "/Divide": RequireDomain(b != 0, "Divide by zero"); return a / b;
                case MATH + "/Power": return Math.Pow(a, b);
                case MATH + "/Modulo": return FlooredMod(a, b);
                case MATH + "/Minimum": return Math.Min(a, b);
                case MATH + "/Maximum": return Math.Max(a, b);
                default: throw Err("No arithmetic primitive for " + typ);
            }
        }

        static bool IsOrderComparison(string typ)
            => typ == TX + "/GreaterThan" || typ == TX + "/LessThan"
            || typ == TX + "/GreaterThanOrEqual" || typ == TX + "/LessThanOrEqual";

        static double BinaryOrder(string typ, double a, double b)
        {
            switch (typ)
            {
                case TX + "/GreaterThan": return Bool(a > b);
                case TX + "/LessThan": return Bool(a < b);
                case TX + "/GreaterThanOrEqual": return Bool(a >= b);
                case TX + "/LessThanOrEqual": return Bool(a <= b);
                default: throw Err("No ordering primitive for " + typ);
            }
        }

        static string IteratorBody(string typ)
        {
            switch (typ)
            {
                case TX + "/ForEach": return "emit";
                case TX + "/ListMap": return "mapBody";
                case TX + "/Filter": return "predicate";
                default: return null;
            }
        }

        static bool IsListFold(string typ)
        {
            switch (typ)
            {
                case TX + "/Count": case TX + "/Sum": case TX + "/Min": case TX + "/Max":
                case TX + "/Average": case TX + "/Join": case TX + "/Reverse":
                    return true;
                default: return false;
            }
        }

        static object ListFold(string typ, List<object> items, ExprNode node)
        {
            string name = typ.Substring(typ.LastIndexOf('/') + 1);
            switch (typ)
            {
                case TX + "/Count": return (double)items.Count;
                case TX + "/Sum":
                {
                    double total = 0;
                    foreach (var el in items) total += RequireNum(el, "Sum");
                    return total;
                }
                case TX + "/Min": case TX + "/Max":
                {
                    if (items.Count == 0) throw Err(name + " on an empty list is undefined; guard with IsSet");
                    double best = RequireNum(items[0], name);
                    for (int i = 1; i < items.Count; i++)
                    {
                        double v = RequireNum(items[i], name);
                        if ((name == "Min" && v < best) || (name == "Max" && v > best)) best = v;
                    }
                    return best;
                }
                case TX + "/Average":
                {
                    if (items.Count == 0) throw Err("Average on an empty list is undefined; guard with IsSet");
                    double total = 0;
                    foreach (var el in items) total += RequireNum(el, "Average");
                    return total / items.Count;
                }
                case TX + "/Join":
                {
                    string sep = node.Get("separator") as string ?? "";
                    var sb = new StringBuilder();
                    for (int i = 0; i < items.Count; i++)
                    {
                        if (i > 0) sb.Append(sep);
                        sb.Append(JoinElement(items[i]));
                    }
                    return sb.ToString();
                }
                case TX + "/Reverse":
                {
                    var outList = new List<object>(items);
                    outList.Reverse();
                    return outList;
                }
                default: throw Err("No list fold for " + typ);
            }
        }

        static bool IsKindPredicate(string typ)
            => typ == TX + "/IsString" || typ == TX + "/IsNumber"
            || typ == TX + "/IsReference" || typ == TX + "/IsList";

        static double KindPredicate(string typ, object v)
        {
            switch (typ)
            {
                case TX + "/IsString": return Bool(v is string);
                case TX + "/IsNumber": return Bool(v is double);
                case TX + "/IsReference": return Bool(v is Ref);
                case TX + "/IsList": return Bool(v is List<object>);
                default: throw Err("No kind predicate for " + typ);
            }
        }

        // -- Matches: the pinned RE2-compatible XSD-regex subset ---------------

        static readonly Regex FlagPrefix = new Regex(@"^\(\?([ims]+)\)");
        static readonly Regex Quantifier = new Regex(@"^\{\d+(,\d*)?\}");
        const string AllowedEscapes = @"dDwWsSbBnrtfv.*+?()[]{}|^$\/";

        // The ASCII word-boundary lookarounds — .NET's native \b is Unicode.
        const string Word = "[0-9A-Za-z_]";
        static readonly string BBoundary =
            "(?:(?<=" + Word + ")(?!" + Word + ")|(?<!" + Word + ")(?=" + Word + "))";
        static readonly string BNonBoundary =
            "(?:(?<=" + Word + ")(?=" + Word + ")|(?<!" + Word + ")(?!" + Word + "))";

        // Code-point `.` — one astral character (a surrogate pair) is ONE dot.
        const string DotNoNewline = "(?:[\uD800-\uDBFF][\uDC00-\uDFFF]|[^\n\uD800-\uDFFF])";
        const string DotAllPoints = "(?:[\uD800-\uDBFF][\uDC00-\uDFFF]|[^\uD800-\uDFFF])";

        /// <summary>The same subset scanner as the reference kernel.</summary>
        internal static void ValidateMatchesPattern(string pattern)
        {
            void Fail(string what)
                => throw Err("Matches pattern is outside the pinned regex subset (" + what + "): " + pattern);

            bool inClass = false;
            int n = pattern.Length;
            for (int i = 0; i < n; i++)
            {
                char c = pattern[i];
                if (c == '\\')
                {
                    if (i + 1 >= n) Fail("trailing backslash");
                    char e = pattern[i + 1];
                    if (e == 'x')
                    {
                        if (i + 2 < n && pattern[i + 2] == '{') Fail(@"\x{…} escape");
                        if (i + 3 >= n || !Uri.IsHexDigit(pattern[i + 2]) || !Uri.IsHexDigit(pattern[i + 3]))
                            Fail(@"\x escape must be \xHH");
                        i += 3;
                        continue;
                    }
                    if (e >= '0' && e <= '9') Fail(@"backreference or octal escape \" + e);
                    if (e == 'p' || e == 'P') Fail(@"unicode property class \" + e + "{…}");
                    if (e == 'k') Fail(@"named backreference \k");
                    if (e == 'u') Fail(@"\u escape");
                    if (e == '-')
                    {
                        if (!inClass) Fail(@"\- outside a character class");
                        i++;
                        continue;
                    }
                    if ((e == 'b' || e == 'B' || e == 'D' || e == 'W' || e == 'S') && inClass)
                        Fail(@"\" + e + " inside a character class");
                    if (AllowedEscapes.IndexOf(e) < 0) Fail(@"escape \" + e);
                    i++;
                    continue;
                }
                if (inClass)
                {
                    if (c == ']') inClass = false;
                    else if (c == '&' && i + 1 < n && pattern[i + 1] == '&') Fail("character-class intersection &&");
                    else if (c == '[' && i + 1 < n && pattern[i + 1] == ':') Fail("POSIX class [[:…:]]");
                    continue;
                }
                if (c == '[') { inClass = true; continue; }
                if (c == '(' && i + 1 < n && pattern[i + 1] == '?')
                {
                    if (i + 2 >= n || pattern[i + 2] != ':') Fail("group construct (?");
                    i += 2;
                    continue;
                }
                if (c == '{')
                {
                    // A bare `{` must start a valid quantifier — the SCANNER enforces
                    // this uniformly (a literal brace is written \{).
                    if (!Quantifier.IsMatch(pattern.Substring(i)))
                        Fail("bare '{' that is not a quantifier (write \\{)");
                }
            }
            if (inClass) Fail("unterminated character class");
        }

        /// <summary>The pinned ASCII expansions plus THIS engine's owed translations:
        /// `.` becomes the surrogate-pair-aware code-point alternation, and \b/\B become
        /// the explicit ASCII-boundary lookarounds.</summary>
        static string ExpandShorthandClasses(string body, bool dotAll)
        {
            var sb = new StringBuilder(body.Length + 32);
            bool inClass = false;
            int n = body.Length;
            for (int i = 0; i < n; i++)
            {
                char c = body[i];
                if (c == '\\')
                {
                    char e = body[i + 1]; // subset-validated: never trailing
                    string expansion = null;
                    if (inClass)
                    {
                        switch (e)
                        {
                            case 'd': expansion = "0-9"; break;
                            case 'w': expansion = "0-9A-Za-z_"; break;
                            case 's': expansion = @" \t\n\r\f\x0B"; break;
                        }
                    }
                    else
                    {
                        switch (e)
                        {
                            case 'd': expansion = "[0-9]"; break;
                            case 'D': expansion = "[^0-9]"; break;
                            case 'w': expansion = "[0-9A-Za-z_]"; break;
                            case 'W': expansion = "[^0-9A-Za-z_]"; break;
                            case 's': expansion = @"[ \t\n\r\f\x0B]"; break;
                            case 'S': expansion = @"[^ \t\n\r\f\x0B]"; break;
                            case 'b': expansion = BBoundary; break;
                            case 'B': expansion = BNonBoundary; break;
                        }
                    }
                    if (expansion != null) sb.Append(expansion);
                    else { sb.Append(c); sb.Append(e); }
                    i++;
                    continue;
                }
                if (!inClass && c == '.')
                {
                    sb.Append(dotAll ? DotAllPoints : DotNoNewline);
                    continue;
                }
                if (!inClass && c == '[') inClass = true;
                else if (inClass && c == ']') inClass = false;
                sb.Append(c);
            }
            return sb.ToString();
        }

        /// <summary>fn:matches semantics: UNANCHORED. Case folding pins to Unicode simple
        /// folding via IgnoreCase|CultureInvariant.</summary>
        static bool MatchesPattern(string input, string pattern)
        {
            string flags = "";
            string body = pattern;
            var m = FlagPrefix.Match(pattern);
            if (m.Success)
            {
                flags = m.Groups[1].Value;
                body = pattern.Substring(m.Length);
            }
            ValidateMatchesPattern(body);
            bool dotAll = flags.IndexOf('s') >= 0;
            var opts = RegexOptions.None;
            if (flags.IndexOf('i') >= 0) opts |= RegexOptions.IgnoreCase | RegexOptions.CultureInvariant;
            if (flags.IndexOf('m') >= 0) opts |= RegexOptions.Multiline;
            // `s` (dotAll) rides the dot TRANSLATION, not RegexOptions.Singleline.
            Regex compiled;
            try
            {
                compiled = new Regex(ExpandShorthandClasses(body, dotAll), opts);
            }
            catch (ArgumentException)
            {
                throw Err("Matches pattern does not compile: " + pattern);
            }
            return compiled.IsMatch(input);
        }

        // -- ordered comparisons (unchanged from v1) ---------------------------

        static string IdentityOf(ExprNode node, object ctx, EvalOptions opts)
        {
            if (node.Type == TX + "/UriLiteral")
            {
                if (node.Get("refTo") is string s && s.Length > 0) return s;
                throw Err("UriLiteral is missing refTo");
            }
            if (opts?.ResolveRef == null)
                throw Err("No resolveRef supplied for identity leaf '" + node.Type + "'");
            return opts.ResolveRef(node, ctx);
        }

        static (double value, string left, string right) FoldOrdered(ExprNode node, object ctx, EvalOptions opts)
        {
            string typ = node.Type;
            if (!(node.Get("viaProperty") is string via) || via.Length == 0)
                throw Err(typ + " is missing viaProperty");
            string left = IdentityOf(Operand(node, "compareLeft"), ctx, opts);
            string right = IdentityOf(Operand(node, "compareRight"), ctx, opts);
            Dictionary<string, List<string>> closure = null;
            opts?.Closures?.TryGetValue(via, out closure);
            if (closure == null) throw Err("No closure supplied for ordering property '" + via + "'");
            double value;
            if (left == right)
            {
                value = Bool(typ == TX + "/IsAtLeast");
            }
            else
            {
                value = Bool(closure.TryGetValue(left, out var reach) && reach.Contains(right));
            }
            return (value, left, right);
        }

        // -- the fold ----------------------------------------------------------

        readonly struct Frame
        {
            public readonly string Name;
            public readonly object Value;
            public Frame(string name, object value) { Name = name; Value = value; }
        }

        static object BoundValue(List<Frame> frames, string name)
        {
            for (int i = frames.Count - 1; i >= 0; i--)
            {
                if (frames[i].Name == name) return frames[i].Value;
            }
            return null;
        }

        /// <summary>Evaluate an expression tree to a value.</summary>
        public static object Evaluate(ExprNode node, object ctx, Resolve resolve, EvalOptions options = null)
            => Go(node, ctx, resolve, options, new List<Frame>());

        static List<object> SourceList(ExprNode node, object ctx, Resolve resolve, EvalOptions options, List<Frame> frames)
        {
            var v = Go(Operand(node, "source"), ctx, resolve, options, frames);
            if (v is List<object> l) return l;
            return new List<object> { v };
        }

        static object Go(ExprNode node, object ctx, Resolve resolve, EvalOptions options, List<Frame> frames)
        {
            string typ = node.Type ?? throw Err("Expression node has no 'type'");

            var ar = OperatorArity(typ);
            if (ar.HasValue)
            {
                var a = ar.Value;
                switch (a.Kind)
                {
                    case "unary":
                    {
                        var x = Go(Operand(node, a.A), ctx, resolve, options, frames);
                        return Unary(typ, RequireNum(x, typ));
                    }
                    case "binary":
                    {
                        var l = Go(Operand(node, a.A), ctx, resolve, options, frames);
                        var r = Go(Operand(node, a.B), ctx, resolve, options, frames);
                        if (typ == TX + "/Equals") return Bool(ValuesEqual(l, r));
                        if (IsOrderComparison(typ))
                        {
                            // Predicate: non-numeric operands fail CLOSED.
                            if (!(l is double ld) || !(r is double rd)) return 0.0;
                            return BinaryOrder(typ, ld, rd);
                        }
                        return BinaryArith(typ, RequireNum(l, typ), RequireNum(r, typ));
                    }
                    case "nary":
                    {
                        if (!(node.Get(a.A) is List<object> items))
                            throw Err(typ + " expects an '" + a.A + "' list");
                        bool isAnd = typ == TX + "/And";
                        foreach (var item in items)
                        {
                            var sub = item as ExprNode ?? throw Err(typ + " operand is not a node");
                            bool v = Truthy(RequireNum(Go(sub, ctx, resolve, options, frames), typ));
                            if (isAnd && !v) return 0.0;
                            if (!isAnd && v) return 1.0;
                        }
                        return Bool(isAnd);
                    }
                    default: // ternary — only Clip today.
                    {
                        double v = RequireNum(Go(Operand(node, a.A), ctx, resolve, options, frames), typ);
                        double lo = RequireNum(Go(Operand(node, a.B), ctx, resolve, options, frames), typ);
                        double hi = RequireNum(Go(Operand(node, a.C), ctx, resolve, options, frames), typ);
                        return Math.Min(Math.Max(v, lo), hi);
                    }
                }
            }

            if (typ == TX + "/Not")
            {
                var x = Go(Operand(node, "operand"), ctx, resolve, options, frames);
                return Bool(!Truthy(RequireNum(x, typ)));
            }

            if (typ == TX + "/IsAtLeast" || typ == TX + "/Dominates")
            {
                return FoldOrdered(node, ctx, options).value;
            }

            if (IsListFold(typ))
            {
                return ListFold(typ, SourceList(node, ctx, resolve, options, frames), node);
            }

            string bodyKey = IteratorBody(typ);
            if (bodyKey != null)
            {
                if (!(node.Get("loopVar") is string loopVar) || loopVar.Length == 0)
                    throw Err(typ + " is missing loopVar");
                var items = SourceList(node, ctx, resolve, options, frames);
                var body = Operand(node, bodyKey);
                var outList = new List<object>();
                foreach (var el in items)
                {
                    frames.Add(new Frame(loopVar, el));
                    object v;
                    try
                    {
                        v = Go(body, ctx, resolve, options, frames);
                    }
                    finally
                    {
                        frames.RemoveAt(frames.Count - 1);
                    }
                    if (typ == TX + "/Filter")
                    {
                        if (Truthy(RequireNum(v, "Filter predicate"))) outList.Add(el);
                    }
                    else if (typ == TX + "/ForEach")
                    {
                        // Flatten one level; an empty list contributes nothing.
                        if (v is List<object> vl) outList.AddRange(vl);
                        else outList.Add(v);
                    }
                    else
                    {
                        outList.Add(v);
                    }
                }
                return outList;
            }

            if (typ == TX + "/Contains")
            {
                var hay = Go(Operand(node, "haystack"), ctx, resolve, options, frames);
                var needle = Go(Operand(node, "needle"), ctx, resolve, options, frames);
                var items = hay as List<object> ?? new List<object> { hay };
                foreach (var el in items)
                {
                    if (ValuesEqual(el, needle)) return 1.0;
                }
                return 0.0;
            }

            if (typ == TX + "/IsSet")
            {
                return Bool(IsSetValue(Go(Operand(node, "checkExpr"), ctx, resolve, options, frames)));
            }

            if (typ == TX + "/ListItemAt")
            {
                var items = SourceList(node, ctx, resolve, options, frames);
                var idx = Go(Operand(node, "itemIndex"), ctx, resolve, options, frames);
                if (!(idx is double d) || d != Math.Floor(d) || d < 0)
                    throw Err("ListItemAt itemIndex must be a non-negative integer");
                int i = (int)d;
                // Past the end is ABSENCE (the empty list); guard with IsSet.
                return i < items.Count ? items[i] : new List<object>();
            }

            if (typ == TX + "/Matches")
            {
                var src = Go(Operand(node, "matchSource"), ctx, resolve, options, frames);
                if (!(src is string s))
                    throw Err("Matches requires a string matchSource, got " + KindOf(src));
                if (!(node.Get("pattern") is string pattern))
                    throw Err("Matches is missing pattern");
                return Bool(MatchesPattern(s, pattern));
            }

            if (IsKindPredicate(typ))
            {
                var x = Go(Operand(node, "kindCheck"), ctx, resolve, options, frames);
                return KindPredicate(typ, x);
            }

            var lit = LiteralValue(node);
            if (lit != null) return lit;

            // A VarRef naming a lexically-enclosing loopVar is the kernel's own
            // bound variable — the ONLY leaf the kernel answers. Everything else
            // is the caller's; recursion from inside resolve re-enters WITHOUT
            // frames.
            if (typ == TX + "/VarRef" && node.Get("varName") is string name)
            {
                var bound = BoundValue(frames, name);
                if (bound != null) return bound;
            }

            return resolve(node, ctx, (n, c) => Go(n, c, resolve, options, new List<Frame>()));
        }

        // -- explain -----------------------------------------------------------

        /// <summary>Evaluate and return the verdict tree. The root's Value is exactly what
        /// <see cref="Evaluate"/> returns for the same inputs; the conformance suite runs
        /// every vector through both and requires agreement.</summary>
        public static TraceNode Explain(ExprNode node, object ctx, Resolve resolve, EvalOptions options = null)
            => Trace(node, ctx, resolve, options, new List<Frame>());

        static TraceNode Leaf(string typ, object value)
            => new TraceNode { Type = typ, Value = value };

        static TraceNode Parent(string typ, object value, params TraceNode[] children)
        {
            var t = new TraceNode { Type = typ, Value = value };
            t.Children.AddRange(children);
            return t;
        }

        static TraceNode Trace(ExprNode node, object ctx, Resolve resolve, EvalOptions options, List<Frame> frames)
        {
            string typ = node.Type ?? throw Err("Expression node has no 'type'");

            var ar = OperatorArity(typ);
            if (ar.HasValue)
            {
                var a = ar.Value;
                switch (a.Kind)
                {
                    case "unary":
                    {
                        var x = Trace(Operand(node, a.A), ctx, resolve, options, frames);
                        return Parent(typ, Unary(typ, RequireNum(x.Value, typ)), x);
                    }
                    case "binary":
                    {
                        var l = Trace(Operand(node, a.A), ctx, resolve, options, frames);
                        var r = Trace(Operand(node, a.B), ctx, resolve, options, frames);
                        object value;
                        if (typ == TX + "/Equals") value = Bool(ValuesEqual(l.Value, r.Value));
                        else if (IsOrderComparison(typ))
                        {
                            if (!(l.Value is double ld) || !(r.Value is double rd)) value = 0.0;
                            else value = BinaryOrder(typ, ld, rd);
                        }
                        else value = BinaryArith(typ, RequireNum(l.Value, typ), RequireNum(r.Value, typ));
                        return Parent(typ, value, l, r);
                    }
                    case "nary":
                    {
                        if (!(node.Get(a.A) is List<object> items))
                            throw Err(typ + " expects an '" + a.A + "' list");
                        bool isAnd = typ == TX + "/And";
                        var t = new TraceNode { Type = typ };
                        foreach (var item in items)
                        {
                            var sub = item as ExprNode ?? throw Err(typ + " operand is not a node");
                            var child = Trace(sub, ctx, resolve, options, frames);
                            t.Children.Add(child);
                            bool v = Truthy(RequireNum(child.Value, typ));
                            if (isAnd && !v) { t.Value = 0.0; return t; }
                            if (!isAnd && v) { t.Value = 1.0; return t; }
                        }
                        t.Value = Bool(isAnd);
                        return t;
                    }
                    default:
                    {
                        var tv = Trace(Operand(node, a.A), ctx, resolve, options, frames);
                        var tlo = Trace(Operand(node, a.B), ctx, resolve, options, frames);
                        var thi = Trace(Operand(node, a.C), ctx, resolve, options, frames);
                        double v = RequireNum(tv.Value, typ);
                        double lo = RequireNum(tlo.Value, typ);
                        double hi = RequireNum(thi.Value, typ);
                        return Parent(typ, Math.Min(Math.Max(v, lo), hi), tv, tlo, thi);
                    }
                }
            }

            if (typ == TX + "/Not")
            {
                var x = Trace(Operand(node, "operand"), ctx, resolve, options, frames);
                return Parent(typ, Bool(!Truthy(RequireNum(x.Value, typ))), x);
            }

            if (typ == TX + "/IsAtLeast" || typ == TX + "/Dominates")
            {
                var r = FoldOrdered(node, ctx, options);
                return new TraceNode { Type = typ, Value = r.value, LeftRef = r.left, RightRef = r.right };
            }

            if (IsListFold(typ))
            {
                var src = Trace(Operand(node, "source"), ctx, resolve, options, frames);
                var items = src.Value as List<object> ?? new List<object> { src.Value };
                return Parent(typ, ListFold(typ, items, node), src);
            }

            string bodyKey = IteratorBody(typ);
            if (bodyKey != null)
            {
                if (!(node.Get("loopVar") is string loopVar) || loopVar.Length == 0)
                    throw Err(typ + " is missing loopVar");
                var src = Trace(Operand(node, "source"), ctx, resolve, options, frames);
                var items = src.Value as List<object> ?? new List<object> { src.Value };
                var body = Operand(node, bodyKey);
                var t = new TraceNode { Type = typ };
                t.Children.Add(src);
                var outList = new List<object>();
                foreach (var el in items)
                {
                    frames.Add(new Frame(loopVar, el));
                    TraceNode bt;
                    try
                    {
                        bt = Trace(body, ctx, resolve, options, frames);
                    }
                    finally
                    {
                        frames.RemoveAt(frames.Count - 1);
                    }
                    t.Children.Add(bt);
                    var v = bt.Value;
                    if (typ == TX + "/Filter")
                    {
                        if (Truthy(RequireNum(v, "Filter predicate"))) outList.Add(el);
                    }
                    else if (typ == TX + "/ForEach")
                    {
                        if (v is List<object> vl) outList.AddRange(vl);
                        else outList.Add(v);
                    }
                    else
                    {
                        outList.Add(v);
                    }
                }
                t.Value = outList;
                return t;
            }

            if (typ == TX + "/Contains")
            {
                var hay = Trace(Operand(node, "haystack"), ctx, resolve, options, frames);
                var needle = Trace(Operand(node, "needle"), ctx, resolve, options, frames);
                var items = hay.Value as List<object> ?? new List<object> { hay.Value };
                double v = 0.0;
                foreach (var el in items)
                {
                    if (ValuesEqual(el, needle.Value)) { v = 1.0; break; }
                }
                return Parent(typ, v, hay, needle);
            }

            if (typ == TX + "/IsSet")
            {
                var x = Trace(Operand(node, "checkExpr"), ctx, resolve, options, frames);
                return Parent(typ, Bool(IsSetValue(x.Value)), x);
            }

            if (typ == TX + "/ListItemAt")
            {
                var src = Trace(Operand(node, "source"), ctx, resolve, options, frames);
                var idx = Trace(Operand(node, "itemIndex"), ctx, resolve, options, frames);
                var items = src.Value as List<object> ?? new List<object> { src.Value };
                if (!(idx.Value is double d) || d != Math.Floor(d) || d < 0)
                    throw Err("ListItemAt itemIndex must be a non-negative integer");
                int i = (int)d;
                object value = i < items.Count ? items[i] : new List<object>();
                return Parent(typ, value, src, idx);
            }

            if (typ == TX + "/Matches")
            {
                var src = Trace(Operand(node, "matchSource"), ctx, resolve, options, frames);
                if (!(src.Value is string s))
                    throw Err("Matches requires a string matchSource, got " + KindOf(src.Value));
                if (!(node.Get("pattern") is string pattern))
                    throw Err("Matches is missing pattern");
                return Parent(typ, Bool(MatchesPattern(s, pattern)), src);
            }

            if (IsKindPredicate(typ))
            {
                var x = Trace(Operand(node, "kindCheck"), ctx, resolve, options, frames);
                return Parent(typ, KindPredicate(typ, x.Value), x);
            }

            var lit = LiteralValue(node);
            if (lit != null) return Leaf(typ, lit);

            if (typ == TX + "/VarRef" && node.Get("varName") is string name)
            {
                var bound = BoundValue(frames, name);
                if (bound != null) return Leaf(typ, bound);
            }

            var rv = resolve(node, ctx, (n, c) => Go(n, c, resolve, options, new List<Frame>()));
            return Leaf(typ, rv);
        }
    }
}
