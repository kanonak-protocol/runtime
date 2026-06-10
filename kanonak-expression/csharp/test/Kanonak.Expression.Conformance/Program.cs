using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text.Json;
using Kanonak.Expression;

// Conformance runner: drives the shared expression parity vectors through the C#
// kanonak-expression port. Each vector's `expr` is evaluated with a `resolve` hook
// that binds tx.VarRef names from the vector's `env` — the demonstration that
// variable binding lives in the caller, not the runtime. Exits non-zero on any
// failure.
//   dotnet run -- <vectors-dir>

class Program
{
    const string VARREF = "kanonak.org/transformations/VarRef";

    static int Main(string[] args)
    {
        string vectorsDir = args.Length > 0 ? args[0] : FindVectorsDir();
        if (vectorsDir == null || !Directory.Exists(vectorsDir))
        {
            Console.Error.WriteLine("vectors directory not found; pass it as the first argument");
            return 2;
        }

        int fail = RunExpression(Path.Combine(vectorsDir, "expression-vectors.json"));
        return fail == 0 ? 0 : 1;
    }

    static int RunExpression(string path)
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(path));
        var vectors = doc.RootElement.GetProperty("vectors");
        int total = 0, pass = 0, fail = 0;

        // A determinism gate that loads zero vectors must never report green —
        // that would silently no-op. Fail loudly before running anything.
        if (vectors.GetArrayLength() == 0)
        {
            Console.Error.WriteLine($"FATAL: 0 vectors loaded from {path} — refusing to report a passing gate");
            return 1;
        }

        foreach (var v in vectors.EnumerateArray())
        {
            total++;
            string id = v.GetProperty("id").GetString();

            // The caller's resolve: tx.VarRef -> env binding; any other leaf is unbound here.
            var env = ReadEnv(v);
            Resolve resolve = (node, ctx, evaluate) =>
            {
                if (node.Type == VARREF)
                {
                    string name = node.Get("varName") as string;
                    if (name == null || !env.ContainsKey(name))
                        throw new ExpressionError($"Unbound variable \"{name}\"");
                    return env[name];
                }
                throw new ExpressionError($"No resolver for leaf '{node.Type}'");
            };

            ExprNode expr = ParseNode(v.GetProperty("expr"));
            bool expectError = v.TryGetProperty("expectError", out var ee) && ee.GetBoolean();

            if (expectError)
            {
                try
                {
                    double got = Expr.Evaluate(expr, env, resolve);
                    fail++; Console.Error.WriteLine($"{id}: expected an error, got {got}");
                }
                catch (ExpressionError)
                {
                    pass++;
                }
                continue;
            }

            double result;
            try
            {
                result = Expr.Evaluate(expr, env, resolve);
            }
            catch (Exception e)
            {
                fail++; Console.Error.WriteLine($"{id}: threw {e.Message}"); continue;
            }

            double expected = v.GetProperty("expected").GetDouble();
            bool ok;
            if (v.TryGetProperty("tolerance", out var tol))
                ok = Math.Abs(result - expected) <= tol.GetDouble();
            else
                ok = result == expected;

            if (ok) pass++;
            else { fail++; Console.Error.WriteLine($"{id}: expected {expected} got {result}"); }
        }

        Console.WriteLine($"expression-vectors: {pass}/{total} pass");
        if (fail > 0) Console.Error.WriteLine($"\n{fail} FAILURES");
        else Console.WriteLine("ALL VECTORS PASS");
        return fail;
    }

    static Dictionary<string, double> ReadEnv(JsonElement v)
    {
        var env = new Dictionary<string, double>();
        if (v.TryGetProperty("env", out var e))
            foreach (var p in e.EnumerateObject())
                env[p.Name] = p.Value.GetDouble();
        return env;
    }

    // Parse a JSON expression node into the ExprNode model. Object children become
    // nested ExprNodes; arrays become List<ExprNode>; scalars stay as boxed values.
    static ExprNode ParseNode(JsonElement el)
    {
        string type = el.GetProperty("type").GetString();
        var fields = new Dictionary<string, object>();
        foreach (var p in el.EnumerateObject())
        {
            if (p.Name == "type") continue;
            fields[p.Name] = ParseValue(p.Value);
        }
        return new ExprNode(type, fields);
    }

    static object ParseValue(JsonElement el)
    {
        switch (el.ValueKind)
        {
            case JsonValueKind.Object:
                return ParseNode(el);
            case JsonValueKind.Array:
                var list = new List<object>();
                foreach (var item in el.EnumerateArray()) list.Add(ParseValue(item));
                return list;
            case JsonValueKind.Number:
                return el.GetDouble();
            case JsonValueKind.True:
                return true;
            case JsonValueKind.False:
                return false;
            case JsonValueKind.String:
                return el.GetString();
            default:
                return null;
        }
    }

    static string FindVectorsDir()
    {
        // Search up from BOTH the assembly location (robust to the dotnet-run
        // working directory — the exe lives under .../csharp/test/.../bin/...,
        // inside the package tree) and the current directory. Used only when no
        // explicit vectors path is given; CI should still pass one.
        foreach (var start in new[] { AppContext.BaseDirectory, Directory.GetCurrentDirectory() })
        {
            for (var dir = new DirectoryInfo(start); dir != null; dir = dir.Parent)
            {
                string c1 = Path.Combine(dir.FullName, "vectors");
                if (File.Exists(Path.Combine(c1, "expression-vectors.json"))) return c1;
                string c2 = Path.Combine(dir.FullName, "kanonak-expression", "vectors");
                if (File.Exists(Path.Combine(c2, "expression-vectors.json"))) return c2;
            }
        }
        return null;
    }
}
