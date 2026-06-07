using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using Kanonak.Canonical;

// Conformance runner: drives the shared golden vectors through the C#
// kanonak-canonical port. Exits non-zero on any failure.
//   dotnet run -- <vectors-dir>

class Program
{
    static int Main(string[] args)
    {
        string vectorsDir = args.Length > 0 ? args[0] : FindVectorsDir();
        if (vectorsDir == null || !Directory.Exists(vectorsDir))
        {
            Console.Error.WriteLine("vectors directory not found; pass it as the first argument");
            return 2;
        }

        int failures = 0;
        failures += RunLexical(Path.Combine(vectorsDir, "lexical-vectors.json"));
        failures += RunFullForm(Path.Combine(vectorsDir, "full-form-vectors.json"));

        Console.WriteLine(failures == 0
            ? "\nALL VECTORS PASS"
            : $"\n{failures} VECTOR(S) FAILED");
        return failures == 0 ? 0 : 1;
    }

    static int RunLexical(string path)
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(path));
        int total = 0, pass = 0, fail = 0;
        foreach (var v in doc.RootElement.GetProperty("vectors").EnumerateArray())
        {
            total++;
            string id = v.GetProperty("id").GetString();
            Carrier carrier = ParseCarrier(v.GetProperty("carrier").GetString());
            string input = v.GetProperty("input").GetString();
            bool expectError = v.TryGetProperty("expectError", out var ee) && ee.GetBoolean();

            try
            {
                string actual = Datatypes.CanonicalScalarLexical(carrier, input);
                if (expectError)
                {
                    fail++; Console.WriteLine($"  FAIL [{id}] expected error, got '{actual}'");
                }
                else
                {
                    string expected = v.GetProperty("expected").GetString();
                    if (actual == expected) pass++;
                    else { fail++; Console.WriteLine($"  FAIL [{id}] expected '{expected}', got '{actual}'"); }
                }
            }
            catch (Exception ex)
            {
                if (expectError) pass++;
                else { fail++; Console.WriteLine($"  FAIL [{id}] threw: {ex.Message}"); }
            }
        }
        Console.WriteLine($"lexical-vectors: {pass}/{total} pass, {fail} fail");
        return fail;
    }

    static int RunFullForm(string path)
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(path));
        int total = 0, pass = 0, fail = 0;
        foreach (var v in doc.RootElement.GetProperty("vectors").EnumerateArray())
        {
            total++;
            string id = v.GetProperty("id").GetString();
            try
            {
                Package pkg = DecodeSubjects(v.GetProperty("input"));
                string form = CanonicalForm.Serialize(pkg);
                string hash = CanonicalForm.Hash(pkg);
                string expForm = v.GetProperty("expectedCanonicalForm").GetString();
                string expHash = v.GetProperty("expectedHash").GetString();
                bool ok = true;
                if (form != expForm) { ok = false; Console.WriteLine($"  FAIL [{id}] form mismatch\n    expected: {expForm}\n    actual:   {form}"); }
                if (hash != expHash) { ok = false; Console.WriteLine($"  FAIL [{id}] hash mismatch expected {expHash} got {hash}"); }
                if (ok) pass++; else fail++;
            }
            catch (Exception ex)
            {
                fail++; Console.WriteLine($"  FAIL [{id}] threw: {ex.Message}");
            }
        }
        Console.WriteLine($"full-form-vectors: {pass}/{total} pass, {fail} fail");
        return fail;
    }

    // -- Input-model decoder (mirrors decode.mjs) ------------------------------

    static Package DecodeSubjects(JsonElement input)
    {
        var subjects = new List<Subject>();
        foreach (var s in input.GetProperty("subjects").EnumerateArray())
        {
            string uri = s.GetProperty("uri").GetString();
            var statements = DecodeStatements(s);
            subjects.Add(new Subject(uri, statements));
        }
        return new Package(subjects);
    }

    static List<Statement> DecodeStatements(JsonElement node)
    {
        var statements = new List<Statement>();
        if (node.TryGetProperty("statements", out var stmts))
        {
            foreach (var st in stmts.EnumerateArray())
            {
                string predicate = st.GetProperty("predicate").GetString();
                statements.Add(new Statement(predicate, DecodeValue(st.GetProperty("value"))));
            }
        }
        return statements;
    }

    static CanonicalValue DecodeValue(JsonElement v)
    {
        if (v.TryGetProperty("lit", out var lit))
        {
            string lexical = lit.GetString();
            Carrier? carrier = CarrierMap.CarrierOf(EntityUri.Parse(v.GetProperty("datatype").GetString()));
            return carrier.HasValue ? (CanonicalValue)new TypedScalar(carrier.Value, lexical) : new RawScalar(lexical);
        }
        if (v.TryGetProperty("raw", out var raw)) return new RawScalar(raw.GetString());
        if (v.TryGetProperty("ref", out var rf)) return new Reference(rf.GetString());
        if (v.TryGetProperty("embed", out var emb))
        {
            string name = emb.TryGetProperty("name", out var nm) ? nm.GetString() : null;
            return new Embedded(name, DecodeStatements(emb));
        }
        if (v.TryGetProperty("list", out var list))
        {
            var items = new List<CanonicalValue>();
            foreach (var item in list.EnumerateArray()) items.Add(DecodeValue(item));
            return new KList(items);
        }
        throw new InvalidOperationException("decode: unknown value shape " + v);
    }

    static Carrier ParseCarrier(string tag)
    {
        switch (tag)
        {
            case "integer": return Carrier.Integer;
            case "decimal": return Carrier.Decimal;
            case "double": return Carrier.Double;
            case "float": return Carrier.Float;
            case "boolean": return Carrier.Boolean;
            case "string": return Carrier.String;
            case "anyURI": return Carrier.AnyUri;
            case "langString": return Carrier.LangString;
            case "dateTime": return Carrier.DateTime;
            case "date": return Carrier.Date;
            case "time": return Carrier.Time;
            case "hexBinary": return Carrier.HexBinary;
            case "base64Binary": return Carrier.Base64Binary;
            default: throw new ArgumentException("unknown carrier tag: " + tag);
        }
    }

    static string FindVectorsDir()
    {
        var dir = new DirectoryInfo(Directory.GetCurrentDirectory());
        while (dir != null)
        {
            string candidate = Path.Combine(dir.FullName, "vectors");
            if (File.Exists(Path.Combine(candidate, "lexical-vectors.json"))) return candidate;
            candidate = Path.Combine(dir.FullName, "packages", "kanonak-canonical", "vectors");
            if (File.Exists(Path.Combine(candidate, "lexical-vectors.json"))) return candidate;
            dir = dir.Parent;
        }
        return null;
    }
}
