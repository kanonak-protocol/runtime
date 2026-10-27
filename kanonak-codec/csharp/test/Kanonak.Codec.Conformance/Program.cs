using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Numerics;
using System.Text.Json;
using Kanonak.Canonical;
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

    /// <summary>The vector files in the codec-vectors-types.json format, run by <see cref="RunTypesFile"/>.</summary>
    static readonly string[] TypesFormatFiles = { "codec-vectors-types.json", "codec-vectors-references.json" };

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
            // Every file in the $types format: the 0.4.0 $types file and the
            // reference-values file (runtime#6) share the runner.
            foreach (var name in TypesFormatFiles)
            {
                string typesPath = FindVectors(name);
                if (typesPath == null)
                {
                    Console.Error.WriteLine(name + " not found; pass the vector files as arguments");
                    return 2;
                }
                int filePassed, fileFailed;
                RunTypesFile(typesPath, out filePassed, out fileFailed);
                Console.WriteLine($"{Path.GetFileName(typesPath)}: {filePassed} passed, {fileFailed} failed");
                passed += filePassed;
                failed += fileFailed;
            }

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

            string compatPath = FindVectors("codec-vectors-compat.json");
            if (compatPath == null)
            {
                Console.Error.WriteLine("codec-vectors-compat.json not found; pass the vector files as arguments");
                return 2;
            }
            int compatPassed, compatFailed;
            RunCompatFile(compatPath, out compatPassed, out compatFailed);
            Console.WriteLine($"{Path.GetFileName(compatPath)}: {compatPassed} passed, {compatFailed} failed");
            passed += compatPassed;
            failed += compatFailed;

            string numbersPath = FindVectors("codec-vectors-json-numbers.json");
            if (numbersPath == null)
            {
                Console.Error.WriteLine("codec-vectors-json-numbers.json not found; pass the vector files as arguments");
                return 2;
            }
            int numbersPassed, numbersFailed;
            RunJsonNumbersFile(numbersPath, out numbersPassed, out numbersFailed);
            Console.WriteLine($"{Path.GetFileName(numbersPath)}: {numbersPassed} passed, {numbersFailed} failed");
            passed += numbersPassed;
            failed += numbersFailed;
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
    // to the same hash. The reference-values file (runtime#6) shares the
    // format: a $ref carrying a URI fragment is rejected on the same three
    // surfaces.
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

            if (c.TryGetProperty("expectHashError", out var hashError))
            {
                // Serializing still carries what hashing refuses (runtime#37).
                string kind = hashError.GetString();
                if (RejectsWithKind(() => Codec.CanonicalForm(nodes, schema, pkg), kind, id)) passed++; else failed++;
                if (RejectsWithKind(() => Codec.ContentHash(nodes, schema, pkg), kind, id)) passed++; else failed++;
            }
            else
            {
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
            }

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

    /// <summary>
    /// The 0.6.1 compatibility file (runtime#28): Deserialize / TypeMatches /
    /// EnumMember / hashing over a node typed at an earlier compatible version
    /// of the schema's package. A rejection must END in the bracketed kind the
    /// vector names — the one part of an error message every port reproduces.
    /// </summary>
    static void RunCompatFile(string vectorsPath, out int passed, out int failed)
    {
        passed = 0;
        failed = 0;
        using var doc = JsonDocument.Parse(File.ReadAllText(vectorsPath));
        JsonElement root = doc.RootElement;

        var schemas = new Dictionary<string, CodecSchema>();
        foreach (var s in root.GetProperty("schemas").EnumerateObject())
            schemas[s.Name] = DecodeSchema(s.Value);
        PackageContext pkg = DecodePkg(root.GetProperty("pkg"));

        foreach (var c in root.GetProperty("deserializeCases").EnumerateArray())
        {
            string id = c.GetProperty("id").GetString();
            CodecSchema schema = schemas[c.GetProperty("schema").GetString()];
            var input = (IReadOnlyDictionary<string, object>)DecodeJson(c.GetProperty("input"));
            bool ok;
            if (c.TryGetProperty("expectError", out var kind))
            {
                ok = RejectsWithKind(() => Codec.Deserialize(input, schema), kind.GetString(), id);
            }
            else
            {
                var got = Codec.Deserialize(input, schema);
                object exp = DecodeJson(c.GetProperty("expected"));
                ok = DeepEquals(got, exp);
                if (!ok) Console.WriteLine($"FAIL [{id}] deserialize\n  got: {Show(got)}\n  exp: {Show(exp)}");
            }
            if (ok) passed++; else failed++;
        }

        foreach (var c in root.GetProperty("typeMatchesCases").EnumerateArray())
        {
            string id = c.GetProperty("id").GetString();
            var node = (IReadOnlyDictionary<string, object>)DecodeJson(c.GetProperty("node"));
            bool got = Codec.TypeMatches(node, c.GetProperty("classUri").GetString(),
                schemas[c.GetProperty("schema").GetString()]);
            bool exp = c.GetProperty("expected").GetBoolean();
            if (got == exp) passed++;
            else { failed++; Console.WriteLine($"FAIL [{id}] typeMatches expected {exp} got {got}"); }
        }

        foreach (var c in root.GetProperty("enumMemberCases").EnumerateArray())
        {
            string id = c.GetProperty("id").GetString();
            EnumMemberMatch m = Codec.EnumMember(schemas[c.GetProperty("schema").GetString()], c.GetProperty("ref").GetString());
            Dictionary<string, object> got = m == null ? null : new Dictionary<string, object>
            {
                ["enumType"] = m.EnumType,
                ["uri"] = m.Uri,
                ["label"] = m.Member.Label,
            };
            object exp = DecodeJson(c.GetProperty("expected"));
            if (DeepEquals(got, exp)) passed++;
            else { failed++; Console.WriteLine($"FAIL [{id}] enumMember\n  got: {Show(got)}\n  exp: {Show(exp)}"); }
        }

        foreach (var c in root.GetProperty("hashCases").EnumerateArray())
        {
            string id = c.GetProperty("id").GetString();
            CodecSchema schema = schemas[c.GetProperty("schema").GetString()];
            var nodes = new List<IReadOnlyDictionary<string, object>>();
            foreach (var n in c.GetProperty("nodes").EnumerateArray())
                nodes.Add((IReadOnlyDictionary<string, object>)DecodeJson(n));
            bool ok;
            if (c.TryGetProperty("expectError", out var kind))
            {
                ok = RejectsWithKind(() => Codec.ContentHash(nodes, schema, pkg), kind.GetString(), id);
            }
            else
            {
                string got = Codec.ContentHash(nodes, schema, pkg);
                string exp = c.GetProperty("expectedHash").GetString();
                ok = got == exp;
                if (!ok) Console.WriteLine($"FAIL [{id}] hash expected {exp} got {got}");
            }
            if (ok) passed++; else failed++;
        }
    }

    /// <summary>
    /// The JSON forms of integer and decimal values (runtime#54): <see cref="Codec.DeserializeExact"/> and
    /// <see cref="Codec.SerializeExact"/> over one field of the file's class. A read case's value must come back in
    /// the type this port gives its datatype, and render to the expected canonical lexical form, item by item and
    /// field by field; a write case's value is built in that type from its canonical lexical form, and its output
    /// compared with the expected JSON. A refusal must end in <c>[json-number-form]</c>.
    /// </summary>
    static void RunJsonNumbersFile(string vectorsPath, out int passed, out int failed)
    {
        passed = 0;
        failed = 0;
        using var doc = JsonDocument.Parse(File.ReadAllText(vectorsPath));
        JsonElement root = doc.RootElement;
        CodecSchema schema = DecodeSchema(root.GetProperty("schema"));
        string classUri = root.GetProperty("class").GetString();
        CodecClass cls = schema.Classes[classUri];

        foreach (var c in root.GetProperty("readCases").EnumerateArray())
        {
            string id = "read " + c.GetProperty("id").GetString();
            string field = c.GetProperty("field").GetString();
            string text = "{\"$type\": " + JsonSerializer.Serialize(classUri) + ", " + JsonSerializer.Serialize(field) + ": " +
                c.GetProperty("json").GetString() + "}";
            IReadOnlyDictionary<string, object> json;
            using (var input = JsonDocument.Parse(text)) json = (IReadOnlyDictionary<string, object>)DecodeJson(input.RootElement);

            bool ok;
            if (c.TryGetProperty("expectError", out var expErr) && expErr.GetBoolean())
            {
                ok = RejectsWithKind(() => Codec.DeserializeExact(json, schema), "json-number-form", id);
            }
            else
            {
                object exp = DecodeJson(c.GetProperty("expected"));
                object got;
                try { got = LexicalOf(Codec.DeserializeExact(json, schema)[field], cls.Props[field], schema); }
                catch (Exception err) { got = "threw: " + err.Message; }
                ok = DeepEquals(got, exp);
                if (!ok) Console.WriteLine($"FAIL [{id}]\n  got: {Show(got)}\n  exp: {Show(exp)}");
            }
            if (ok) passed++; else failed++;
        }

        foreach (var c in root.GetProperty("writeCases").EnumerateArray())
        {
            string id = "write " + c.GetProperty("id").GetString();
            string field = c.GetProperty("field").GetString();
            var node = new Dictionary<string, object>
            {
                ["$type"] = classUri,
                [field] = PortValue(c.GetProperty("value"), cls.Props[field], schema),
            };

            bool ok;
            if (c.TryGetProperty("expectError", out var expErr) && expErr.GetBoolean())
            {
                ok = RejectsWithKind(() => Codec.SerializeExact(node, schema), "json-number-form", id);
            }
            else
            {
                object exp;
                using (var expected = JsonDocument.Parse(c.GetProperty("json").GetString())) exp = DecodeJson(expected.RootElement);
                object got;
                try { got = Codec.SerializeExact(node, schema)[field]; }
                catch (Exception err) { got = "threw: " + err.Message; }
                ok = DeepEquals(got, exp);
                if (!ok) Console.WriteLine($"FAIL [{id}]\n  got: {Show(got)}\n  exp: {Show(exp)}");
            }
            if (ok) passed++; else failed++;
        }
    }

    const string Xsd = "kanonak.org/core-xsd/";

    /// <summary>The type this port gives a value of the prop's datatype, read or written.</summary>
    static Type PortType(CodecProp prop)
    {
        switch (Coordinate.LenientVersionlessKey(prop.Datatype))
        {
            case Xsd + "decimal":
                return typeof(string);
            case Xsd + "long":
            case Xsd + "int":
            case Xsd + "short":
            case Xsd + "byte":
            case Xsd + "unsignedInt":
            case Xsd + "unsignedShort":
            case Xsd + "unsignedByte":
                return typeof(long);
            case Xsd + "unsignedLong":
                return typeof(ulong);
            default:
                return typeof(BigInteger);
        }
    }

    /// <summary>
    /// A read value rendered as its canonical lexical form, item by item and field by field; a scalar of any type
    /// but <see cref="PortType"/> renders as a mismatch that names both.
    /// </summary>
    static object LexicalOf(object value, CodecProp prop, CodecSchema schema)
    {
        if (value is IReadOnlyDictionary<string, object> map)
        {
            CodecClass embedded = schema.Classes[prop.Range];
            var fields = new Dictionary<string, object>();
            foreach (var kv in map) fields[kv.Key] = LexicalOf(kv.Value, embedded.Props[kv.Key], schema);
            return fields;
        }
        if (value is System.Collections.IList list)
        {
            var items = new List<object>();
            foreach (var item in list) items.Add(LexicalOf(item, prop, schema));
            return items;
        }
        Type expected = PortType(prop);
        if (value?.GetType() != expected) return $"a {value?.GetType().Name ?? "null"}, not a {expected.Name}";
        return Convert.ToString(value, CultureInfo.InvariantCulture);
    }

    /// <summary>
    /// A write case's value in <see cref="PortType"/>, from its canonical lexical form. A value past a
    /// <see cref="long"/> or a <see cref="ulong"/> is a <see cref="BigInteger"/>, which <see cref="Codec.SerializeExact"/>
    /// takes for any wide integer, so its range check is what refuses it.
    /// </summary>
    static object PortValue(JsonElement value, CodecProp prop, CodecSchema schema)
    {
        if (value.ValueKind == JsonValueKind.Object)
        {
            CodecClass embedded = schema.Classes[prop.Range];
            var fields = new Dictionary<string, object>();
            foreach (var p in value.EnumerateObject()) fields[p.Name] = PortValue(p.Value, embedded.Props[p.Name], schema);
            return fields;
        }
        if (value.ValueKind == JsonValueKind.Array)
        {
            var items = new List<object>();
            foreach (var item in value.EnumerateArray()) items.Add(PortValue(item, prop, schema));
            return items;
        }
        string lexical = value.GetString();
        Type type = PortType(prop);
        if (type == typeof(string)) return lexical;
        if (type == typeof(long) && long.TryParse(lexical, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out long l))
            return l;
        if (type == typeof(ulong) && ulong.TryParse(lexical, NumberStyles.None, CultureInfo.InvariantCulture, out ulong u))
            return u;
        return BigInteger.Parse(lexical, CultureInfo.InvariantCulture);
    }

    /// <summary>Whether <paramref name="run"/> throws with a message ending in <c>[kind]</c>; reports otherwise.</summary>
    static bool RejectsWithKind(Action run, string kind, string id)
    {
        try
        {
            run();
            Console.WriteLine($"FAIL [{id}] expected a [{kind}] rejection, got a value");
            return false;
        }
        catch (Exception err)
        {
            bool ok = err.Message.EndsWith("[" + kind + "]", StringComparison.Ordinal);
            if (!ok) Console.WriteLine($"FAIL [{id}] expected [{kind}], got: {err.Message}");
            return ok;
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
