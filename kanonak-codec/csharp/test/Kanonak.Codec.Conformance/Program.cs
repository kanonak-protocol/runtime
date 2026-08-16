using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using Kanonak.Codec;

// Conformance runner: drives the shared codec vector files (the 0.1.0 base vectors
// plus the 0.2.0 embedded-value vectors) through the C# kanonak-codec port and
// asserts the canonical form, content hash, and normalized-JSON serialize all match
// the authoritative (TypeScript-generated) expected values. Serialize is compared
// structurally (semantically), not by JSON text or key order.
//   dotnet run -- <path-to-codec-vectors.json> [<more-vector-files>...]

class Program
{
    /// <summary>The shared vector files every conformant port must pass.</summary>
    /// <summary>Durable VERSIONED member-key formation: publisher/package@version/name.</summary>
    static readonly System.Text.RegularExpressions.Regex MemberUri =
        new System.Text.RegularExpressions.Regex(@"^[^/]+/[^/@]+@\d+\.\d+\.\d+/[^/]+$");

    static readonly string[] VectorFiles = { "codec-vectors.json", "codec-vectors-embedded.json" };

    static int Main(string[] args)
    {
        var vectorsPaths = new List<string>();
        if (args.Length > 0)
        {
            vectorsPaths.AddRange(args);
        }
        else
        {
            foreach (var name in VectorFiles)
            {
                string found = FindVectors(name);
                if (found == null)
                {
                    Console.Error.WriteLine(name + " not found; pass the vector files as arguments");
                    return 2;
                }
                vectorsPaths.Add(found);
            }
        }

        int passed = 0, failed = 0;
        foreach (var vectorsPath in vectorsPaths)
        {
            if (!File.Exists(vectorsPath))
            {
                Console.Error.WriteLine(vectorsPath + " not found");
                return 2;
            }
            int filePassed, fileFailed;
            RunFile(vectorsPath, out filePassed, out fileFailed);
            Console.WriteLine($"{Path.GetFileName(vectorsPath)}: {filePassed} passed, {fileFailed} failed");
            passed += filePassed;
            failed += fileFailed;
        }

        if (args.Length == 0)
        {
            string typesPath = FindVectors("codec-vectors-types.json");
            if (typesPath == null)
            {
                Console.Error.WriteLine("codec-vectors-types.json not found; pass the vector files as arguments");
                return 2;
            }
            int filePassed, fileFailed;
            RunTypesFile(typesPath, out filePassed, out fileFailed);
            Console.WriteLine($"{Path.GetFileName(typesPath)}: {filePassed} passed, {fileFailed} failed");
            passed += filePassed;
            failed += fileFailed;

            string enumsPath = FindVectors("codec-vectors-enums.json");
            if (enumsPath == null)
            {
                Console.Error.WriteLine("codec-vectors-enums.json not found; pass the vector files as arguments");
                return 2;
            }
            int enumsPassed, enumsFailed;
            RunEnumsFile(enumsPath, out enumsPassed, out enumsFailed);
            Console.WriteLine($"{Path.GetFileName(enumsPath)}: {enumsPassed} passed, {enumsFailed} failed");
            passed += enumsPassed;
            failed += enumsFailed;
        }

        Console.WriteLine($"\n{passed} passed, {failed} failed");
        return failed == 0 ? 0 : 1;
    }

    // The 0.4.0 multi-typed-subjects file (runtime#10). Beyond the standard
    // form/hash/serialize checks it exercises the $types contract: expectError
    // cases must be rejected on ALL THREE surfaces — Serialize (the producer
    // fails at emit time), Deserialize (the reader rejects, never repairs), and
    // canonicalization — and positive cases must round-trip:
    // Deserialize(Serialize(x)) preserves $types exactly and re-canonicalizes
    // to the same hash.
    static void RunTypesFile(string vectorsPath, out int passed, out int failed)
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(vectorsPath));
        var root = doc.RootElement;
        CodecSchema schema = DecodeSchema(root.GetProperty("schema"));

        passed = 0; failed = 0;
        foreach (var c in root.GetProperty("cases").EnumerateArray())
        {
            string id = c.GetProperty("id").GetString();
            PackageContext pkg = DecodePkg(c.GetProperty("pkg"));

            var nodes = new List<IReadOnlyDictionary<string, object>>();
            foreach (var n in c.GetProperty("nodes").EnumerateArray())
                nodes.Add((IReadOnlyDictionary<string, object>)DecodeJson(n));

            if (c.TryGetProperty("expectError", out var expErr) && expErr.GetBoolean())
            {
                bool ok = true;
                if (!Rejects(() => Codec.CanonicalForm(nodes, schema, pkg)))
                { ok = false; Console.WriteLine($"FAIL [{id}] expected canonicalize to reject, it did not"); }
                if (!Rejects(() => { foreach (var n in nodes) Codec.Serialize(n); }))
                { ok = false; Console.WriteLine($"FAIL [{id}] expected serialize to reject, it did not"); }
                if (!Rejects(() => { foreach (var n in nodes) Codec.Deserialize(n, schema); }))
                { ok = false; Console.WriteLine($"FAIL [{id}] expected deserialize to reject, it did not"); }
                if (ok) passed++; else failed++;
                continue;
            }

            string form = Codec.CanonicalForm(nodes, schema, pkg);
            string expForm = c.GetProperty("expectedCanonicalForm").GetString();
            if (form == expForm) passed++;
            else { failed++; Console.WriteLine($"FAIL [{id}] canonical form\n  got: {form}\n  exp: {expForm}"); }

            string hash = Codec.ContentHash(nodes, schema, pkg);
            string expHash = c.GetProperty("expectedHash").GetString();
            if (hash == expHash) passed++;
            else { failed++; Console.WriteLine($"FAIL [{id}] hash\n  got: {hash}\n  exp: {expHash}"); }

            var expSer = c.GetProperty("expectedSerialize");
            var roundTripped = new List<IReadOnlyDictionary<string, object>>();
            bool serOk = true;
            for (int i = 0; i < nodes.Count; i++)
            {
                var wire = Codec.Serialize(nodes[i]);
                object exp = DecodeJson(expSer[i]);
                if (!DeepEquals(wire, exp))
                { serOk = false; Console.WriteLine($"FAIL [{id}] serialize[{i}]\n  got: {Show(wire)}\n  exp: {Show(exp)}"); }

                var back = Codec.Deserialize(wire, schema);
                if (!DeepEquals(Codec.Serialize(back), exp))
                { serOk = false; Console.WriteLine($"FAIL [{id}] round-trip serialize[{i}] mismatch"); }
                roundTripped.Add(back);
            }
            if (serOk) passed++; else failed++;

            string rtHash = Codec.ContentHash(roundTripped, schema, pkg);
            if (rtHash == expHash) passed++;
            else { failed++; Console.WriteLine($"FAIL [{id}] round-trip hash\n  got: {rtHash}\n  exp: {expHash}"); }
        }
    }

    static bool Rejects(Action run)
    {
        try { run(); return false; }
        catch (ArgumentException) { return true; }
    }

    static void RunFile(string vectorsPath, out int passed, out int failed)
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(vectorsPath));
        var root = doc.RootElement;
        CodecSchema schema = DecodeSchema(root.GetProperty("schema"));

        passed = 0; failed = 0;
        foreach (var c in root.GetProperty("cases").EnumerateArray())
        {
            string id = c.GetProperty("id").GetString();
            PackageContext pkg = DecodePkg(c.GetProperty("pkg"));

            var nodes = new List<IReadOnlyDictionary<string, object>>();
            foreach (var n in c.GetProperty("nodes").EnumerateArray())
                nodes.Add((IReadOnlyDictionary<string, object>)DecodeJson(n));

            // Canonical form.
            string form = Codec.CanonicalForm(nodes, schema, pkg);
            string expForm = c.GetProperty("expectedCanonicalForm").GetString();
            if (form == expForm) passed++;
            else { failed++; Console.WriteLine($"FAIL [{id}] canonical form\n  got: {form}\n  exp: {expForm}"); }

            // Content hash.
            string hash = Codec.ContentHash(nodes, schema, pkg);
            string expHash = c.GetProperty("expectedHash").GetString();
            if (hash == expHash) passed++;
            else { failed++; Console.WriteLine($"FAIL [{id}] hash\n  got: {hash}\n  exp: {expHash}"); }

            // Serialize each node + deserialize round-trip.
            var expSer = c.GetProperty("expectedSerialize");
            for (int i = 0; i < nodes.Count; i++)
            {
                var got = Codec.Serialize(nodes[i]);
                object exp = DecodeJson(expSer[i]);
                if (DeepEquals(got, exp)) passed++;
                else { failed++; Console.WriteLine($"FAIL [{id}] serialize[{i}]\n  got: {Show(got)}\n  exp: {Show(exp)}"); }

                var back = Codec.Deserialize(got, schema);
                if (Equals(GetVal(back, "$type"), GetVal(nodes[i], "$type"))) passed++;
                else { failed++; Console.WriteLine($"FAIL [{id}] deserialize[{i}] $type"); }
            }
        }

    }

    /// <summary>
    /// The 0.5.0 enumerations file (<c>enums</c>, runtime#21). Beyond the
    /// standard form/hash/serialize checks it pins the enumeration contract:
    /// the schema parses with <c>enums</c> keyed by durable VERSIONED URIs at
    /// both levels (a versionless key would look right and miss every lookup);
    /// an enumeration STANDS ALONE, with no <c>classes</c> twin; <c>enums</c>
    /// and an enum-ranged <c>range</c> are canonicalization-INERT; and
    /// expectError cases are rejected at CANONICALIZATION. Unlike the $types
    /// file's all-three-surfaces contract this violation is schema-DEPENDENT:
    /// Serialize is schema-free and Deserialize does not recurse into embedded
    /// values, so canonicalization is the only surface that can see it.
    /// </summary>
    static void RunEnumsFile(string vectorsPath, out int passed, out int failed)
    {
        passed = 0;
        failed = 0;
        using var doc = JsonDocument.Parse(File.ReadAllText(vectorsPath));
        JsonElement root = doc.RootElement;
        CodecSchema schema = DecodeSchema(root.GetProperty("schema"));

        // -- structural assertions on the schema itself --
        bool shapeOk = schema.Enums != null && schema.Enums.Count > 0;
        if (!shapeOk) Console.WriteLine("FAIL [enums] schema carries no enums");
        if (schema.Enums != null)
        {
            foreach (var kv in schema.Enums)
            {
                if (kv.Key != kv.Value.TypeUri)
                { shapeOk = false; Console.WriteLine($"FAIL [enums] key {kv.Key} != typeUri {kv.Value.TypeUri}"); }
                if (schema.Classes.ContainsKey(kv.Key))
                { shapeOk = false; Console.WriteLine($"FAIL [enums] {kv.Key} must NOT also appear in classes"); }
                if (kv.Value.Members.Count == 0)
                { shapeOk = false; Console.WriteLine($"FAIL [enums] {kv.Key} declares no members"); }
                foreach (var memberUri in kv.Value.Members.Keys)
                {
                    if (!MemberUri.IsMatch(memberUri))
                    { shapeOk = false; Console.WriteLine($"FAIL [enums] member key {memberUri} is not a versioned durable URI"); }
                }
            }
        }
        if (shapeOk) passed++; else failed++;

        // -- cases --
        foreach (var c in root.GetProperty("cases").EnumerateArray())
        {
            string id = c.GetProperty("id").GetString();
            var pkg = DecodePkg(c.GetProperty("pkg"));
            var nodes = new List<IReadOnlyDictionary<string, object>>();
            foreach (var n in c.GetProperty("nodes").EnumerateArray())
                nodes.Add((IReadOnlyDictionary<string, object>)DecodeJson(n));

            if (c.TryGetProperty("expectError", out var expErr) && expErr.GetBoolean())
            {
                if (Rejects(() => Codec.CanonicalForm(nodes, schema, pkg))) passed++;
                else { failed++; Console.WriteLine($"FAIL [{id}] expected canonicalization to reject, it did not"); }
                continue;
            }

            bool ok = true;
            string form = Codec.CanonicalForm(nodes, schema, pkg);
            string expForm = c.GetProperty("expectedCanonicalForm").GetString();
            if (form != expForm) { ok = false; Console.WriteLine($"FAIL [{id}] canonical form mismatch"); }

            string hash = Codec.ContentHash(nodes, schema, pkg);
            string expHash = c.GetProperty("expectedHash").GetString();
            if (hash != expHash) { ok = false; Console.WriteLine($"FAIL [{id}] hash got {hash} exp {expHash}"); }

            var expSer = c.GetProperty("expectedSerialize");
            for (int i = 0; i < nodes.Count; i++)
            {
                if (!DeepEquals(Codec.Serialize(nodes[i]), DecodeJson(expSer[i])))
                { ok = false; Console.WriteLine($"FAIL [{id}] serialize[{i}] mismatch"); }
            }
            if (ok) passed++; else failed++;
        }
    }

    // -- Vector decoders -------------------------------------------------------

    // Delegates to the production parser rather than duplicating it: this was
    // a line-for-line copy of CodecSchema.FromJson and would have silently
    // produced enum-free schemas when 0.5.0 added `enums`. One parser, so the
    // harness cannot drift from what consumers actually run.
    static CodecSchema DecodeSchema(JsonElement s) => CodecSchema.FromJson(s.GetRawText());

    static PackageContext DecodePkg(JsonElement p)
    {
        var pkg = new PackageContext
        {
            Publisher = p.GetProperty("publisher").GetString(),
            PackageName = p.GetProperty("packageName").GetString(),
            Version = p.GetProperty("version").GetString(),
        };
        if (p.TryGetProperty("label", out var l) && l.ValueKind != JsonValueKind.Null) pkg.Label = l.GetString();
        return pkg;
    }

    /// <summary>Decode a JsonElement into the CLR shapes the codec consumes
    /// (Dictionary / List / string / bool / long / decimal / null).</summary>
    static object DecodeJson(JsonElement e)
    {
        switch (e.ValueKind)
        {
            case JsonValueKind.Object:
                var map = new Dictionary<string, object>();
                foreach (var prop in e.EnumerateObject()) map[prop.Name] = DecodeJson(prop.Value);
                return map;
            case JsonValueKind.Array:
                var list = new List<object>();
                foreach (var item in e.EnumerateArray()) list.Add(DecodeJson(item));
                return list;
            case JsonValueKind.String: return e.GetString();
            case JsonValueKind.True: return true;
            case JsonValueKind.False: return false;
            case JsonValueKind.Null: return null;
            case JsonValueKind.Number:
                if (e.TryGetInt64(out long l)) return l;
                return e.GetDecimal();
            default:
                throw new InvalidOperationException("unsupported JSON kind " + e.ValueKind);
        }
    }

    // -- Structural equality ---------------------------------------------------

    static bool DeepEquals(object a, object b)
    {
        if (a == null || b == null) return a == null && b == null;

        if (a is IReadOnlyDictionary<string, object> am && b is IReadOnlyDictionary<string, object> bm)
        {
            if (am.Count != bm.Count) return false;
            foreach (var kv in am)
            {
                if (!bm.TryGetValue(kv.Key, out var bv)) return false;
                if (!DeepEquals(kv.Value, bv)) return false;
            }
            return true;
        }

        if (a is System.Collections.IList al && b is System.Collections.IList bl)
        {
            if (al.Count != bl.Count) return false;
            for (int i = 0; i < al.Count; i++)
                if (!DeepEquals(al[i], bl[i])) return false;
            return true;
        }

        if (IsNumber(a) && IsNumber(b))
            return Convert.ToDecimal(a) == Convert.ToDecimal(b);

        return a.Equals(b);
    }

    static bool IsNumber(object o)
        => o is byte || o is sbyte || o is short || o is ushort || o is int || o is uint
           || o is long || o is ulong || o is float || o is double || o is decimal;

    static object GetVal(IReadOnlyDictionary<string, object> m, string k)
        => m.TryGetValue(k, out var v) ? v : null;

    static string Show(object o) => JsonSerializer.Serialize(o);

    static string FindVectors(string fileName)
    {
        var dir = new DirectoryInfo(Directory.GetCurrentDirectory());
        while (dir != null)
        {
            string candidate = Path.Combine(dir.FullName, "vectors", fileName);
            if (File.Exists(candidate)) return candidate;
            candidate = Path.Combine(dir.FullName, "kanonak-codec", "vectors", fileName);
            if (File.Exists(candidate)) return candidate;
            dir = dir.Parent;
        }
        return null;
    }
}
