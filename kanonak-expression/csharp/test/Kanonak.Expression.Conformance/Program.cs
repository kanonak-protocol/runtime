using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using Kanonak.Expression;

// Conformance runner: drives the shared expression parity vectors through the C#
// kanonak-expression port — BOTH files: expression-vectors.json (v1 — passes
// UNCHANGED under the v2 kernel; the numeric-regression gate) and
// expression-vectors-2.json (the value-domain extension). env bindings and
// expected are Values (numbers, strings, arrays, {"ref": …} objects). Every
// vector runs through Evaluate AND Explain and their values must agree; vectors
// with a `trace` assert the verdict tree structurally. Exits non-zero on any
// failure.
//   dotnet run -- <vectors-dir>

class Program
{
    const string VARREF = "kanonak.org/transformations/VarRef";
    const string PROPERTY_READ = "kanonak.org/transformations/PropertyRead";

    static int Main(string[] args)
    {
        string vectorsDir = args.Length > 0 ? args[0] : FindVectorsDir();
        if (vectorsDir == null || !Directory.Exists(vectorsDir))
        {
            Console.Error.WriteLine("vectors directory not found; pass it as the first argument");
            return 2;
        }

        int fail = RunFile(Path.Combine(vectorsDir, "expression-vectors.json"));
        fail += RunFile(Path.Combine(vectorsDir, "expression-vectors-2.json"));
        fail += RunAlignFile(Path.Combine(vectorsDir, "expression-alignment-vectors.json"));
        if (fail == 0) Console.WriteLine("ALL VECTORS PASS");
        return fail == 0 ? 0 : 1;
    }

    static string FindVectorsDir()
    {
        var dir = new DirectoryInfo(Directory.GetCurrentDirectory());
        while (dir != null)
        {
            string candidate = Path.Combine(dir.FullName, "vectors");
            if (Directory.Exists(candidate)
                && File.Exists(Path.Combine(candidate, "expression-vectors.json")))
            {
                return candidate;
            }
            dir = dir.Parent;
        }
        return null;
    }

    /// JSON node -> ExprNode: nested objects become ExprNodes, arrays of objects
    /// become lists of ExprNodes, scalars stay raw.
    static ExprNode NodeOf(JsonElement e)
    {
        string type = e.GetProperty("type").GetString();
        var fields = new Dictionary<string, object>();
        foreach (var p in e.EnumerateObject())
        {
            if (p.Name == "type") continue;
            fields[p.Name] = FieldOf(p.Value);
        }
        return new ExprNode(type, fields);
    }

    static object FieldOf(JsonElement e)
    {
        switch (e.ValueKind)
        {
            case JsonValueKind.Object: return NodeOf(e);
            case JsonValueKind.Array:
            {
                var list = new List<object>();
                foreach (var item in e.EnumerateArray()) list.Add(FieldOf(item));
                return list;
            }
            case JsonValueKind.String: return e.GetString();
            case JsonValueKind.Number: return e.GetDouble();
            case JsonValueKind.True: return true;
            case JsonValueKind.False: return false;
            default: return null;
        }
    }

    /// JSON -> Value, the vector-file encoding. Booleans normalize to 1/0.
    static object ValueOf(JsonElement e)
    {
        switch (e.ValueKind)
        {
            case JsonValueKind.Number: return e.GetDouble();
            case JsonValueKind.String: return e.GetString();
            case JsonValueKind.True: return 1.0;
            case JsonValueKind.False: return 0.0;
            case JsonValueKind.Array:
            {
                var list = new List<object>();
                foreach (var item in e.EnumerateArray()) list.Add(ValueOf(item));
                return list;
            }
            case JsonValueKind.Object:
                if (e.TryGetProperty("ref", out var r) && r.ValueKind == JsonValueKind.String)
                    return new Ref(r.GetString());
                break;
        }
        throw new InvalidOperationException("unrepresentable vector value");
    }

    /// Deep Value equality — the vector-comparison rule (lists structural).
    static bool DeepEqual(object a, object b)
    {
        if (a is List<object> al && b is List<object> bl)
        {
            if (al.Count != bl.Count) return false;
            for (int i = 0; i < al.Count; i++)
            {
                if (!DeepEqual(al[i], bl[i])) return false;
            }
            return true;
        }
        if (a is List<object> || b is List<object>) return false;
        if (a is double x && b is double y) return x == y;
        if (a is string s && b is string t) return s == t;
        if (a is Ref p && b is Ref q) return p.Equals(q);
        return false;
    }

    sealed class Ctx
    {
        public Dictionary<string, object> Env = new Dictionary<string, object>();
        public Dictionary<string, string> RefEnv = new Dictionary<string, string>();
        /// <summary>A host graph for the caller's tx.PropertyRead leaf: ref URI -> property -> Value.</summary>
        public Dictionary<string, Dictionary<string, object>> Graph = new Dictionary<string, Dictionary<string, object>>();
    }

    // The caller's resolve: tx.VarRef -> env binding; tx.PropertyRead -> a host graph read
    // (the documented caller-leaf shape — the kernel never touches a graph). PropertyRead is
    // the reference engine's convention, pinned by the vectors so every port's harness
    // agrees: readSource is evaluated through the handed-back evaluate (so a loopVar bound by
    // an enclosing iterator is visible — runtime#25) and MUST yield a ref; then
    // graph[ref][readProp] is absent -> the empty list, several values -> a list, one -> itself.
    static object ResolveHook(ExprNode node, object ctxRaw, Func<ExprNode, object, object> evaluate)
    {
        var ctx = (Ctx)ctxRaw;
        if (node.Type == VARREF)
        {
            string name = node.Get("varName") as string;
            if (name != null && ctx.Env.TryGetValue(name, out var v)) return v;
            throw new ExpressionError($"Unbound variable \"{name}\"");
        }
        if (node.Type == PROPERTY_READ)
        {
            var source = evaluate(node.Get("readSource") as ExprNode, ctx);
            if (!(source is Ref r)) throw new ExpressionError($"PropertyRead over a non-ref: {source}");
            string prop = node.Get("readProp") as string;
            if (prop != null && ctx.Graph.TryGetValue(r.Uri, out var props) && props.TryGetValue(prop, out var values)) return values;
            return new List<object>();
        }
        throw new ExpressionError($"No resolver for leaf '{node.Type}'");
    }

    static string ResolveRefHook(ExprNode node, object ctxRaw)
    {
        var ctx = (Ctx)ctxRaw;
        if (node.Type == VARREF)
        {
            string name = node.Get("varName") as string;
            if (name != null && ctx.RefEnv.TryGetValue(name, out var v)) return v;
            throw new ExpressionError($"Unbound reference \"{name}\"");
        }
        throw new ExpressionError($"No reference resolver for leaf '{node.Type}'");
    }

    static bool TraceMatches(TraceNode got, JsonElement want)
    {
        if (want.GetProperty("type").GetString() != got.Type) return false;
        if (!want.TryGetProperty("value", out var wv) || !DeepEqual(got.Value, ValueOf(wv))) return false;
        string wantLeft = want.TryGetProperty("leftRef", out var wl) ? wl.GetString() : null;
        string wantRight = want.TryGetProperty("rightRef", out var wr) ? wr.GetString() : null;
        if (wantLeft != got.LeftRef || wantRight != got.RightRef) return false;
        int wantCount = want.TryGetProperty("children", out var wc) ? wc.GetArrayLength() : 0;
        if (wantCount != got.Children.Count) return false;
        int i = 0;
        if (wantCount > 0)
        {
            foreach (var child in wc.EnumerateArray())
            {
                if (!TraceMatches(got.Children[i++], child)) return false;
            }
        }
        return true;
    }

    static int RunFile(string path)
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(path));
        var vectors = doc.RootElement.GetProperty("vectors");
        string name = Path.GetFileName(path);
        int total = 0, pass = 0, fail = 0;

        // A determinism gate that loads zero vectors must never report green.
        if (vectors.GetArrayLength() == 0)
        {
            Console.Error.WriteLine($"FATAL: 0 vectors loaded from {path} — refusing to report a passing gate");
            return 1;
        }

        foreach (var v in vectors.EnumerateArray())
        {
            total++;
            string id = v.GetProperty("id").GetString();
            var expr = NodeOf(v.GetProperty("expr"));

            var ctx = new Ctx();
            if (v.TryGetProperty("env", out var env))
            {
                foreach (var p in env.EnumerateObject()) ctx.Env[p.Name] = ValueOf(p.Value);
            }
            if (v.TryGetProperty("refEnv", out var refEnv))
            {
                foreach (var p in refEnv.EnumerateObject()) ctx.RefEnv[p.Name] = p.Value.GetString();
            }
            if (v.TryGetProperty("graph", out var graph))
            {
                foreach (var node in graph.EnumerateObject())
                {
                    var props = new Dictionary<string, object>();
                    foreach (var p in node.Value.EnumerateObject()) props[p.Name] = ValueOf(p.Value);
                    ctx.Graph[node.Name] = props;
                }
            }

            Dictionary<string, Dictionary<string, List<string>>> closures = null;
            if (v.TryGetProperty("closures", out var cl))
            {
                closures = new Dictionary<string, Dictionary<string, List<string>>>();
                foreach (var prop in cl.EnumerateObject())
                {
                    var inner = new Dictionary<string, List<string>>();
                    foreach (var member in prop.Value.EnumerateObject())
                    {
                        var reach = new List<string>();
                        foreach (var m in member.Value.EnumerateArray()) reach.Add(m.GetString());
                        inner[member.Name] = reach;
                    }
                    closures[prop.Name] = inner;
                }
            }
            var options = new EvalOptions { Closures = closures, ResolveRef = ResolveRefHook };

            bool expectError = v.TryGetProperty("expectError", out var ee) && ee.GetBoolean();
            if (expectError)
            {
                bool evalThrew = false, explainThrew = false;
                try { Expr.Evaluate(expr, ctx, ResolveHook, options); } catch (ExpressionError) { evalThrew = true; }
                try { Expr.Explain(expr, ctx, ResolveHook, options); } catch (ExpressionError) { explainThrew = true; }
                if (evalThrew && explainThrew) pass++;
                else { fail++; Console.WriteLine($"  FAIL [{name}/{id}] expected an error from Evaluate AND Explain"); }
                continue;
            }

            object got;
            TraceNode trace;
            try
            {
                got = Expr.Evaluate(expr, ctx, ResolveHook, options);
                trace = Expr.Explain(expr, ctx, ResolveHook, options);
            }
            catch (ExpressionError e)
            {
                fail++; Console.WriteLine($"  FAIL [{name}/{id}] threw: {e.Message}");
                continue;
            }

            object expected = ValueOf(v.GetProperty("expected"));
            bool ok;
            if (v.TryGetProperty("tolerance", out var tol))
            {
                ok = got is double g && expected is double e2 && Math.Abs(g - e2) <= tol.GetDouble();
            }
            else
            {
                ok = DeepEqual(got, expected);
            }
            if (!ok)
            {
                fail++; Console.WriteLine($"  FAIL [{name}/{id}] expected {expected}, got {got}");
                continue;
            }
            if (!DeepEqual(trace.Value, got))
            {
                fail++; Console.WriteLine($"  FAIL [{name}/{id}] explain value {trace.Value} != evaluate value {got}");
                continue;
            }
            if (v.TryGetProperty("trace", out var wantTrace) && !TraceMatches(trace, wantTrace))
            {
                fail++; Console.WriteLine($"  FAIL [{name}/{id}] trace mismatch");
                continue;
            }
            pass++;
        }
        Console.WriteLine($"{name}: {pass}/{total} pass" + (fail == 0 ? "" : $", {fail} fail"));
        return fail;
    }

    /// <summary>Structural equality of an aligned tree against the vector's expected JSON tree.</summary>
    static bool AlignedMatches(AlignedNode got, JsonElement want)
    {
        string typ = want.GetProperty("type").GetString();
        if (got.Expr.Type != typ || got.Trace.Type != typ) return false;
        string wantOperand = want.TryGetProperty("operand", out var wo) ? wo.GetString() : null;
        if (got.Operand != wantOperand) return false;
        int? wantIndex = want.TryGetProperty("index", out var wi) ? wi.GetInt32() : (int?)null;
        if (got.Index != wantIndex) return false;
        bool hasWantEl = want.TryGetProperty("element", out var wel);
        if ((got.Element == null) != !hasWantEl) return false;
        if (got.Element != null && (got.Element.LoopVar != wel.GetProperty("loopVar").GetString()
            || !DeepEqual(got.Element.Value, ValueOf(wel.GetProperty("value"))))) return false;
        if (!want.TryGetProperty("value", out var wv) || !DeepEqual(got.Trace.Value, ValueOf(wv))) return false;
        var wantChildren = want.TryGetProperty("children", out var wc) ? wc : default;
        int wantCount = wantChildren.ValueKind == JsonValueKind.Array ? wantChildren.GetArrayLength() : 0;
        if (wantCount != got.Children.Count) return false;
        int i = 0;
        if (wantCount > 0)
            foreach (var w in wantChildren.EnumerateArray()) { if (!AlignedMatches(got.Children[i++], w)) return false; }
        return true;
    }

    /// <summary>The alignment vectors: explain expr, then Align — the pairing must be exactly the expected tree,
    /// or an error where one is expected. Returns the failure count.</summary>
    static int RunAlignFile(string path)
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(path));
        var vectors = doc.RootElement.GetProperty("vectors");
        string name = Path.GetFileName(path);
        int total = 0, pass = 0, fail = 0;
        foreach (var v in vectors.EnumerateArray())
        {
            total++;
            string id = v.GetProperty("id").GetString();
            var expr = NodeOf(v.GetProperty("expr"));
            var ctx = new Ctx();
            if (v.TryGetProperty("env", out var env))
                foreach (var p in env.EnumerateObject()) ctx.Env[p.Name] = ValueOf(p.Value);
            if (v.TryGetProperty("refEnv", out var refEnv))
                foreach (var p in refEnv.EnumerateObject()) ctx.RefEnv[p.Name] = p.Value.GetString();
            if (v.TryGetProperty("graph", out var graph))
                foreach (var node in graph.EnumerateObject())
                {
                    var props = new Dictionary<string, object>();
                    foreach (var p in node.Value.EnumerateObject()) props[p.Name] = ValueOf(p.Value);
                    ctx.Graph[node.Name] = props;
                }
            Dictionary<string, Dictionary<string, List<string>>> closures = null;
            if (v.TryGetProperty("closures", out var cl))
            {
                closures = new Dictionary<string, Dictionary<string, List<string>>>();
                foreach (var prop in cl.EnumerateObject())
                {
                    var inner = new Dictionary<string, List<string>>();
                    foreach (var member in prop.Value.EnumerateObject())
                    {
                        var reach = new List<string>();
                        foreach (var x in member.Value.EnumerateArray()) reach.Add(x.GetString());
                        inner[member.Name] = reach;
                    }
                    closures[prop.Name] = inner;
                }
            }
            var options = new EvalOptions { Closures = closures, ResolveRef = ResolveRefHook };
            TraceNode trace;
            try { trace = Expr.Explain(expr, ctx, ResolveHook, options); }
            catch (ExpressionError e) { fail++; Console.WriteLine($"  FAIL [{name}/{id}] explain: {e.Message}"); continue; }
            if (v.TryGetProperty("expectError", out var ee) && ee.GetBoolean())
            {
                var target = v.TryGetProperty("alignExpr", out var ae) ? NodeOf(ae) : expr;
                try { Expr.Align(target, trace); fail++; Console.WriteLine($"  FAIL [{name}/{id}] expected align to reject the trace"); }
                catch (ExpressionError) { pass++; }
                continue;
            }
            AlignedNode got;
            try { got = Expr.Align(expr, trace); }
            catch (ExpressionError e) { fail++; Console.WriteLine($"  FAIL [{name}/{id}] align: {e.Message}"); continue; }
            if (!AlignedMatches(got, v.GetProperty("expected"))) { fail++; Console.WriteLine($"  FAIL [{name}/{id}] alignment mismatch"); continue; }
            pass++;
        }
        Console.WriteLine($"{name}: {pass}/{total} pass");
        return fail;
    }
}
