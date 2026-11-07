// Typed-surface conformance: hand-written GENERATED-STYLE classes for the
// embedded-vectors probe schema, driven through the KanonakNode / Ref<T> /
// TypedNodes binding, asserted against the SAME golden vectors the node
// contract is gated by (codec-vectors.json + codec-vectors-embedded.json).
// This file is also the executable spec for what an SDK generator must emit:
// classes extend KanonakNode, object properties are Ref<T>/List<Ref<T>>,
// wire names ride [JsonPropertyName].
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Numerics;
using System.Text.Json;
using System.Text.Json.Serialization;
using Kanonak.Codec;

// -- Generated-style properties for the JSON forms (runtime#54) ---------------

// The embedded Amount of codec-vectors-json-numbers.json, its decimal named with its datatype's converter.
sealed class BindingAmount
{
    [JsonPropertyName("value"), JsonConverter(typeof(DecimalJsonConverter))] public string Value { get; set; }
}

// core-owl's cardinality, an xsd:nonNegativeInteger, the field that asked for the binding.
sealed class BindingRestriction
{
    [JsonPropertyName("cardinality"), JsonConverter(typeof(NonNegativeIntegerJsonConverter))] public BigInteger Cardinality { get; set; }
}

// A list of xsd:integer, and of xsd:positiveInteger, each named with the list converter of its datatype's converter,
// as a generated class names a list property (runtime#70).
sealed class BindingCounts
{
    [JsonPropertyName("counts"), JsonConverter(typeof(JsonFormListConverter<IntegerJsonConverter, BigInteger>))] public List<BigInteger> Counts { get; set; }
}

sealed class BindingPositives
{
    [JsonPropertyName("values"), JsonConverter(typeof(JsonFormListConverter<PositiveIntegerJsonConverter, BigInteger>))] public List<BigInteger> Values { get; set; }
}

// -- An embedded value into an interface (runtime#72) -------------------------
// Generated code types an object property Ref<IShape> when it may hold any class that implements IShape. The
// interface's own converter chooses the class by the node's $type, as code that knows the classes does.

[JsonConverter(typeof(ShapeJsonConverter))]
interface IShape { }

sealed class Circle : KanonakNode, IShape
{
    [JsonPropertyName("radius")] public long Radius { get; set; }
}

sealed class Square : KanonakNode, IShape
{
    [JsonPropertyName("side")] public long Side { get; set; }
}

sealed class ShapeJsonConverter : JsonConverter<IShape>
{
    public const string SquareType = "probe.example.com/shapes@1.0.0/Square";

    public override IShape Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        using var doc = JsonDocument.ParseValue(ref reader);
        bool square = doc.RootElement.TryGetProperty("$type", out var type) && type.GetString() == SquareType;
        return square ? doc.RootElement.Deserialize<Square>(options) : doc.RootElement.Deserialize<Circle>(options);
    }

    public override void Write(Utf8JsonWriter writer, IShape value, JsonSerializerOptions options)
        => JsonSerializer.Serialize(writer, (object)value, value.GetType(), options);
}

sealed class Drawing
{
    [JsonPropertyName("shape")] public Ref<IShape> Shape { get; set; }
    [JsonPropertyName("shapes")] public List<Ref<IShape>> Shapes { get; set; }
}

interface IUnconverted { }

abstract class AbstractUnconverted { }

sealed class Sketch
{
    [JsonPropertyName("thing")] public Ref<IUnconverted> Thing { get; set; }
    [JsonPropertyName("other")] public Ref<AbstractUnconverted> Other { get; set; }
}

// -- Generated-style model for probe.example.com/schema@1.0.0 ----------------

sealed class Order : KanonakNode
{
    [JsonPropertyName("note")] public string Note { get; set; }
    [JsonPropertyName("items")] public List<Ref<LineItem>> Items { get; set; }
    [JsonPropertyName("customer")] public List<Ref<Customer>> Customer { get; set; }
}

// Same $type, single-valued customer — the wire/hash contract is carried by
// $type + [JsonPropertyName], not the C# class name, so a second fixture shape
// exercises the bare (non-list) embedded form.
sealed class OrderSingleCustomer : KanonakNode
{
    [JsonPropertyName("note")] public string Note { get; set; }
    [JsonPropertyName("customer")] public Ref<Customer> Customer { get; set; }
}

sealed class LineItem : KanonakNode
{
    [JsonPropertyName("sku")] public string Sku { get; set; }
    [JsonPropertyName("qty")] public long? Qty { get; set; }
}

sealed class Customer : KanonakNode
{
    [JsonPropertyName("name")] public string CustomerName { get; set; }
    [JsonPropertyName("address")] public List<Ref<Address>> Address { get; set; }
}

sealed class Address : KanonakNode
{
    [JsonPropertyName("city")] public string City { get; set; }
}

sealed class Person : KanonakNode
{
    [JsonPropertyName("name")] public string PersonName { get; set; }
}

sealed class Account : KanonakNode
{
    [JsonPropertyName("accountCode")] public string AccountCode { get; set; }
    [JsonPropertyName("seats")] public long? Seats { get; set; }
    [JsonPropertyName("rate")] public decimal? Rate { get; set; }
    [JsonPropertyName("active")] public bool? Active { get; set; }
    [JsonPropertyName("owner")] public Ref<Person> Owner { get; set; }
    [JsonPropertyName("tags")] public List<string> Tags { get; set; }
}

// -- Generated-style model for the $types vectors (0.4.0, runtime#10) --------
// The multi-typed set rides the model via KanonakNode.Types ($types envelope
// only — deliberately no unprefixed wire name, because an ontology can model a
// property literally named "types").

sealed class DefResource : KanonakNode
{
    [JsonPropertyName("note")] public string Note { get; set; }
}

sealed class Bundle : KanonakNode
{
    [JsonPropertyName("parts")] public List<Ref<PartDef>> Parts { get; set; }
}

// Same $type, single-valued parts — exercises the bare embedded form.
sealed class BundleSinglePart : KanonakNode
{
    [JsonPropertyName("parts")] public Ref<PartDef> Parts { get; set; }
}

sealed class PartDef : KanonakNode
{
    [JsonPropertyName("size")] public long? Size { get; set; }
}

// -- Generated-style model for the reference-values vectors (runtime#6) ------

sealed class Doc : KanonakNode
{
    [JsonPropertyName("title")] public string Title { get; set; }
    [JsonPropertyName("cites")] public Ref<Doc> Cites { get; set; }
    [JsonPropertyName("sections")] public List<Ref<Section>> Sections { get; set; }
}

sealed class Section : KanonakNode
{
    [JsonPropertyName("heading")] public string Heading { get; set; }
    [JsonPropertyName("cites")] public Ref<Doc> Cites { get; set; }
}

static class Program
{
    const string SCHEMA = "probe.example.com/schema@1.0.0";
    const string DATA = "probe.example.com/data@1.0.0";

    static int _fails;

    static int Main()
    {
        string vectorsDir = FindVectorsDir();
        var embedded = JsonDocument.Parse(File.ReadAllText(Path.Combine(vectorsDir, "codec-vectors-embedded.json")));
        var basic = JsonDocument.Parse(File.ReadAllText(Path.Combine(vectorsDir, "codec-vectors.json")));
        var embSchema = CodecSchema.FromJson(embedded.RootElement.GetProperty("schema").GetRawText());
        var basicSchema = CodecSchema.FromJson(basic.RootElement.GetProperty("schema").GetRawText());
        var embPkg = new PackageContext
        {
            Publisher = "probe.example.com", PackageName = "data", Version = "1.0.0",
            Label = "Embedded Probe Data",
        };

        // Each typed fixture must reproduce the golden expected values of the
        // NAMED vector case — the typed path and the node path are one contract.

        Check(embedded, "embedded-named-in-list", embSchema, embPkg, new KanonakNode[]
        {
            new Order
            {
                Id = DATA + "/o1", Type = SCHEMA + "/Order", Note = "A",
                Items = new List<Ref<LineItem>>
                {
                    Ref<LineItem>.Embed(new LineItem { Sku = "X", Qty = 1 }, "first"),
                },
            },
        });

        Check(embedded, "embedded-unnamed-positional", embSchema, embPkg, new KanonakNode[]
        {
            new Order
            {
                Id = DATA + "/o1", Type = SCHEMA + "/Order", Note = "A",
                Items = new List<Ref<LineItem>>
                {
                    Ref<LineItem>.Embed(new LineItem { Sku = "X", Qty = 1 }),
                },
            },
        });

        Check(embedded, "embedded-explicit-type", embSchema, embPkg, new KanonakNode[]
        {
            new Order
            {
                Id = DATA + "/o1", Type = SCHEMA + "/Order", Note = "A",
                Items = new List<Ref<LineItem>>
                {
                    Ref<LineItem>.Embed(
                        new LineItem { Type = SCHEMA + "/LineItem", Sku = "X", Qty = 1 }, "first"),
                },
            },
        });

        Check(embedded, "embedded-list-order", embSchema, embPkg, new KanonakNode[]
        {
            new Order
            {
                Id = DATA + "/o1", Type = SCHEMA + "/Order",
                Items = new List<Ref<LineItem>>
                {
                    Ref<LineItem>.Embed(new LineItem { Sku = "X", Qty = 1 }, "a"),
                    Ref<LineItem>.Embed(new LineItem { Sku = "Y", Qty = 2 }, "b"),
                },
            },
        });

        Check(embedded, "embedded-nested", embSchema, embPkg, new KanonakNode[]
        {
            new Order
            {
                Id = DATA + "/o1", Type = SCHEMA + "/Order",
                Customer = new List<Ref<Customer>>
                {
                    Ref<Customer>.Embed(new Customer
                    {
                        CustomerName = "Ada",
                        Address = new List<Ref<Address>>
                        {
                            Ref<Address>.Embed(new Address { City = "Austin" }, "home"),
                        },
                    }, "cust"),
                },
            },
        });

        Check(embedded, "single-embedded-bare", embSchema, embPkg, new KanonakNode[]
        {
            new OrderSingleCustomer
            {
                Id = DATA + "/o1", Type = SCHEMA + "/Order",
                Customer = Ref<Customer>.Embed(new Customer { CustomerName = "Ada" }, "cust"),
            },
        });

        Check(embedded, "empty-list-emits-nothing", embSchema, embPkg, new KanonakNode[]
        {
            new Order
            {
                Id = DATA + "/o1", Type = SCHEMA + "/Order", Note = "A",
                Items = new List<Ref<LineItem>>(),
            },
        }, checkSerialize: false); // typed empty list serializes as [] — node path proves canonical parity

        // The 0.1.0 basic case through the typed path: references + scalar list.
        var basicPkg = new PackageContext
        {
            Publisher = "probe.example.com", PackageName = "data", Version = "1.0.0",
            Label = "Codec Probe Data",
        };
        Check(basic, "basic-scalars-ref-list", basicSchema, basicPkg, new KanonakNode[]
        {
            new Person { Id = DATA + "/p1", Type = SCHEMA + "/Person", PersonName = "Alice" },
            new Account
            {
                Id = DATA + "/a1", Type = SCHEMA + "/Account",
                AccountCode = "paul", Seats = 5, Rate = 1.5m, Active = true,
                Owner = Ref<Person>.To(DATA + "/p1"),
                Tags = new List<string> { "x", "y" },
            },
        });

        // Ref-by-instance: Ref<T>.To(node) resolves through the target's Id.
        var alice = new Person { Id = DATA + "/p1", Type = SCHEMA + "/Person", PersonName = "Alice" };
        Check(basic, "basic-scalars-ref-list", basicSchema, basicPkg, new KanonakNode[]
        {
            alice,
            new Account
            {
                Id = DATA + "/a1", Type = SCHEMA + "/Account",
                AccountCode = "paul", Seats = 5, Rate = 1.5m, Active = true,
                Owner = Ref<Person>.To(alice),
                Tags = new List<string> { "x", "y" },
            },
        });

        // The 0.4.0 $types cases (runtime#10) through the typed path.
        var types = JsonDocument.Parse(File.ReadAllText(Path.Combine(vectorsDir, "codec-vectors-types.json")));
        var typesSchema = CodecSchema.FromJson(types.RootElement.GetProperty("schema").GetRawText());
        var typesPkg = new PackageContext
        {
            Publisher = "probe.example.com", PackageName = "data", Version = "1.0.0",
            Label = "Types Probe Data",
        };

        Check(types, "covered-redundant-set", typesSchema, typesPkg, new KanonakNode[]
        {
            new DefResource
            {
                Id = DATA + "/w1", Type = SCHEMA + "/ClassDef",
                Types = new List<string> { SCHEMA + "/AnnotatedDef", SCHEMA + "/ClassDef" },
                Note = "A",
            },
        });

        Check(types, "embedded-multi-typed-named", typesSchema, typesPkg, new KanonakNode[]
        {
            new BundleSinglePart
            {
                Id = DATA + "/b1", Type = SCHEMA + "/Bundle",
                Parts = Ref<PartDef>.Embed(new PartDef
                {
                    Type = SCHEMA + "/PartDef",
                    Types = new List<string> { SCHEMA + "/PartDef", SCHEMA + "/SealedDef" },
                    Size = 2,
                }, "first"),
            },
        });

        Check(types, "types-in-list-items", typesSchema, typesPkg, new KanonakNode[]
        {
            new Bundle
            {
                Id = DATA + "/b1", Type = SCHEMA + "/Bundle",
                Parts = new List<Ref<PartDef>>
                {
                    Ref<PartDef>.Embed(new PartDef
                    {
                        Type = SCHEMA + "/PartDef",
                        Types = new List<string> { SCHEMA + "/PartDef", SCHEMA + "/SealedDef" },
                        Size = 1,
                    }, "a"),
                    Ref<PartDef>.Embed(new PartDef { Type = SCHEMA + "/PartDef", Size = 2 }, "b"),
                },
            },
        });

        // The reference-values cases (runtime#6) through the typed path: a
        // Ref<T>.To(uri) carrying a URI fragment is rejected on the way to
        // canonicalization, at the top level and inside an embedded value.
        var refs = JsonDocument.Parse(File.ReadAllText(Path.Combine(vectorsDir, "codec-vectors-references.json")));
        var refsSchema = CodecSchema.FromJson(refs.RootElement.GetProperty("schema").GetRawText());
        var refsPkg = new PackageContext
        {
            Publisher = "probe.example.com", PackageName = "data", Version = "1.0.0",
            Label = "References Probe Data",
        };

        Check(refs, "named-reference-versioned", refsSchema, refsPkg, new KanonakNode[]
        {
            NewDoc(Ref<Doc>.To(DATA + "/d2"), null),
        });
        Check(refs, "named-reference-in-embedded", refsSchema, refsPkg, new KanonakNode[]
        {
            NewDoc(null, new Section { Heading = "Intro", Cites = Ref<Doc>.To(DATA + "/d2") }),
        });
        CheckRejects("fragment-reference-rejected", refsSchema, refsPkg, "fragment-reference", new KanonakNode[]
        {
            NewDoc(Ref<Doc>.To(DATA + "/d2#sections[0]"), null),
        });
        CheckRejects("fragment-reference-in-embedded-rejected", refsSchema, refsPkg, "fragment-reference", new KanonakNode[]
        {
            NewDoc(null, new Section { Heading = "Intro", Cites = Ref<Doc>.To(DATA + "/d2#intro") }),
        });

        RunJsonFormBinding(vectorsDir);
        RunRefIntoInterface();

        Console.WriteLine();
        if (_fails > 0) { Console.Error.WriteLine(_fails + " TYPED CHECK(S) FAILED"); return 1; }
        Console.WriteLine("ALL TYPED CHECKS PASS");
        return 0;
    }

    static void Check(
        JsonDocument vectors, string caseId, CodecSchema schema, PackageContext pkg,
        KanonakNode[] typed, bool checkSerialize = true)
    {
        JsonElement theCase = default;
        bool found = false;
        foreach (var c in vectors.RootElement.GetProperty("cases").EnumerateArray())
        {
            if (c.GetProperty("id").GetString() == caseId) { theCase = c; found = true; break; }
        }
        if (!found) { Fail(caseId, "vector case not found"); return; }

        string expForm = theCase.GetProperty("expectedCanonicalForm").GetString();
        string expHash = theCase.GetProperty("expectedHash").GetString();

        try
        {
            string form = TypedNodes.CanonicalForm(typed, schema, pkg);
            string hash = TypedNodes.ContentHash(typed, schema, pkg);
            if (form != expForm) Fail(caseId, "canonical form mismatch\n  expected " + expForm + "\n  got      " + form);
            else if (hash != expHash) Fail(caseId, "hash expected " + expHash + " got " + hash);
            else Console.WriteLine("PASS  " + caseId);
        }
        catch (Exception ex)
        {
            Fail(caseId, ex.GetType().Name + ": " + ex.Message);
        }
    }

    static Doc NewDoc(Ref<Doc> cites, Section intro) => new Doc
    {
        Id = DATA + "/d1", Type = SCHEMA + "/Doc", Title = "One", Cites = cites,
        Sections = intro == null ? null : new List<Ref<Section>> { Ref<Section>.Embed(intro, "intro") },
    };

    // The typed instances must be rejected on their way to canonicalization,
    // with a message ending in [kind].
    static void CheckRejects(
        string label, CodecSchema schema, PackageContext pkg, string kind, KanonakNode[] typed)
    {
        try
        {
            TypedNodes.ContentHash(typed, schema, pkg);
            Fail(label, "expected a [" + kind + "] rejection, got a hash");
        }
        catch (ArgumentException ex)
        {
            if (ex.Message.EndsWith("[" + kind + "]", StringComparison.Ordinal)) Console.WriteLine("PASS  " + label);
            else Fail(label, "expected [" + kind + "], got: " + ex.Message);
        }
    }

    // -- The System.Text.Json binding of the JSON forms (runtime#54) ---------------
    // Each read and write case of codec-vectors-json-numbers.json through JsonSerializer, each value through its
    // datatype's converter. Every refusal names [json-number-form]; a write case whose value its CLR type can't
    // hold, such as a long past 64 bits, is refused by the type itself.

    static JsonSerializerOptions With(JsonConverter converter) => new JsonSerializerOptions { Converters = { converter } };

    static string Lexical(object value) =>
        JsonSerializer.Serialize(Convert.ToString(value, CultureInfo.InvariantCulture));

    static string ReadAs(string field, string json)
    {
        switch (field)
        {
            case "integer": return Lexical(JsonSerializer.Deserialize<BigInteger>(json, With(new IntegerJsonConverter())));
            case "nonNegativeInteger": return Lexical(JsonSerializer.Deserialize<BigInteger>(json, With(new NonNegativeIntegerJsonConverter())));
            case "positiveInteger": return Lexical(JsonSerializer.Deserialize<BigInteger>(json, With(new PositiveIntegerJsonConverter())));
            case "nonPositiveInteger": return Lexical(JsonSerializer.Deserialize<BigInteger>(json, With(new NonPositiveIntegerJsonConverter())));
            case "negativeInteger": return Lexical(JsonSerializer.Deserialize<BigInteger>(json, With(new NegativeIntegerJsonConverter())));
            case "long": return Lexical(JsonSerializer.Deserialize<long>(json, With(new LongJsonConverter())));
            case "unsignedLong": return Lexical(JsonSerializer.Deserialize<ulong>(json, With(new UnsignedLongJsonConverter())));
            case "decimal": return Lexical(JsonSerializer.Deserialize<string>(json, With(new DecimalJsonConverter())));
            case "int": return Lexical(JsonSerializer.Deserialize<int>(json, With(new IntJsonConverter())));
            case "short": return Lexical(JsonSerializer.Deserialize<short>(json, With(new ShortJsonConverter())));
            case "byte": return Lexical(JsonSerializer.Deserialize<sbyte>(json, With(new ByteJsonConverter())));
            case "unsignedInt": return Lexical(JsonSerializer.Deserialize<uint>(json, With(new UnsignedIntJsonConverter())));
            case "unsignedShort": return Lexical(JsonSerializer.Deserialize<ushort>(json, With(new UnsignedShortJsonConverter())));
            case "unsignedByte": return Lexical(JsonSerializer.Deserialize<byte>(json, With(new UnsignedByteJsonConverter())));
            case "counts":
                return JsonSerializer.Serialize(JsonSerializer.Deserialize<BindingCounts>("{\"counts\": " + json + "}").Counts
                    .Select(n => n.ToString(CultureInfo.InvariantCulture)).ToList());
            case "amount":
                return JsonSerializer.Serialize(new Dictionary<string, string> { ["value"] = JsonSerializer.Deserialize<BindingAmount>(json).Value });
            default: throw new InvalidOperationException("no binding type for field " + field);
        }
    }

    static string WriteAs(string field, JsonElement value)
    {
        string text = value.ValueKind == JsonValueKind.String ? value.GetString() : null;
        switch (field)
        {
            case "integer": return JsonSerializer.Serialize(BigInteger.Parse(text, CultureInfo.InvariantCulture), With(new IntegerJsonConverter()));
            case "nonNegativeInteger": return JsonSerializer.Serialize(BigInteger.Parse(text, CultureInfo.InvariantCulture), With(new NonNegativeIntegerJsonConverter()));
            case "positiveInteger": return JsonSerializer.Serialize(BigInteger.Parse(text, CultureInfo.InvariantCulture), With(new PositiveIntegerJsonConverter()));
            case "long": return JsonSerializer.Serialize(long.Parse(text, CultureInfo.InvariantCulture), With(new LongJsonConverter()));
            case "unsignedLong": return JsonSerializer.Serialize(ulong.Parse(text, CultureInfo.InvariantCulture), With(new UnsignedLongJsonConverter()));
            case "decimal": return JsonSerializer.Serialize(text, With(new DecimalJsonConverter()));
            case "int": return JsonSerializer.Serialize(int.Parse(text, CultureInfo.InvariantCulture), With(new IntJsonConverter()));
            case "byte": return JsonSerializer.Serialize(sbyte.Parse(text, CultureInfo.InvariantCulture), With(new ByteJsonConverter()));
            case "unsignedShort": return JsonSerializer.Serialize(ushort.Parse(text, CultureInfo.InvariantCulture), With(new UnsignedShortJsonConverter()));
            case "counts":
                return JsonDocument.Parse(JsonSerializer.Serialize(new BindingCounts
                {
                    Counts = value.EnumerateArray().Select(v => BigInteger.Parse(v.GetString(), CultureInfo.InvariantCulture)).ToList(),
                })).RootElement.GetProperty("counts").GetRawText();
            case "amount": return JsonSerializer.Serialize(new BindingAmount { Value = value.GetProperty("value").GetString() });
            default: throw new InvalidOperationException("no binding type for field " + field);
        }
    }

    static string Normal(string json) => JsonSerializer.Serialize(JsonDocument.Parse(json).RootElement);

    static void RunJsonFormBinding(string vectorsDir)
    {
        var doc = JsonDocument.Parse(File.ReadAllText(Path.Combine(vectorsDir, "codec-vectors-json-numbers.json")));
        int total = 0, failedBefore = _fails;
        foreach (var c in doc.RootElement.GetProperty("readCases").EnumerateArray())
        {
            total++;
            string id = "binding read " + c.GetProperty("id").GetString();
            bool expectError = c.TryGetProperty("expectError", out var e) && e.GetBoolean();
            string got = null, error = null;
            try { got = ReadAs(c.GetProperty("field").GetString(), c.GetProperty("json").GetString()); }
            catch (JsonException ex) { error = ex.Message; }
            if (expectError && error == null) Fail(id, "expected a refusal, got " + got);
            else if (expectError && !error.Contains("[json-number-form]")) Fail(id, "refused without [json-number-form]: " + error);
            else if (!expectError && error != null) Fail(id, "refused: " + error);
            else if (!expectError && Normal(got) != Normal(c.GetProperty("expected").GetRawText()))
                Fail(id, "expected " + c.GetProperty("expected").GetRawText() + ", got " + got);
        }
        foreach (var c in doc.RootElement.GetProperty("writeCases").EnumerateArray())
        {
            total++;
            string id = "binding write " + c.GetProperty("id").GetString();
            string want = c.TryGetProperty("json", out var j) ? j.GetString() : null;
            string written = null, error = null;
            try { written = WriteAs(c.GetProperty("field").GetString(), c.GetProperty("value")); }
            catch (Exception ex) when (ex is JsonException || ex is OverflowException) { error = ex.Message; }
            if (want == null && error == null) Fail(id, "expected a refusal, wrote " + written);
            else if (want != null && error != null) Fail(id, "refused: " + error);
            else if (want != null && Normal(written) != Normal(want)) Fail(id, "expected " + want + ", got " + written);
        }
        // A list property's converter reads each item at its own datatype, and refuses what isn't a list of it.
        foreach (var (id, read) in new (string, Func<object>)[]
        {
            ("binding list: an item outside its datatype", () => JsonSerializer.Deserialize<BindingPositives>("{\"values\": [\"1\", \"0\"]}")),
            ("binding list: a value that isn't an array", () => JsonSerializer.Deserialize<BindingCounts>("{\"counts\": \"5\"}")),
            ("binding list: a null item", () => JsonSerializer.Deserialize<BindingCounts>("{\"counts\": [\"1\", null]}")),
        })
        {
            total++;
            try { read(); Fail(id, "expected a refusal"); }
            catch (JsonException ex) { if (!ex.Message.Contains("[json-number-form]")) Fail(id, "refused without [json-number-form]: " + ex.Message); }
        }
        total++;
        var positives = JsonSerializer.Deserialize<BindingPositives>("{\"values\": [\"1\", \"9007199254740993\"]}");
        if (positives.Values.Count != 2 || positives.Values[1] != BigInteger.Parse("9007199254740993", CultureInfo.InvariantCulture))
            Fail("binding list: positiveInteger", "read as " + string.Join(", ", positives.Values));
        total++;
        try { JsonSerializer.Serialize(new BindingPositives { Values = new List<BigInteger> { BigInteger.One, BigInteger.Zero } }); Fail("binding list: writing an item outside its datatype", "expected a refusal"); }
        catch (JsonException ex) { if (!ex.Message.Contains("[json-number-form]")) Fail("binding list: writing an item outside its datatype", "refused without [json-number-form]: " + ex.Message); }
        total++;
        var restriction = JsonSerializer.Deserialize<BindingRestriction>("{\"cardinality\": \"1\"}");
        if (restriction.Cardinality != BigInteger.One) Fail("binding cardinality", "\"1\" read as " + restriction.Cardinality);
        // A number token is judged by its digits, so a fraction its double rounds away is still refused.
        foreach (var (field, json, expected) in new (string, string, string)[]
        {
            ("integer", "1.0000000000000000001", null),
            ("integer", "1.00000000000000000000000000001", null),
            ("integer", "1e-500", null),
            ("int", "1e-500", null),
            ("integer", "0e-500", "\"0\""),
            ("integer", "-0.0", "\"0\""),
        })
        {
            total++;
            string id = "binding read edge " + field + " " + json;
            string got = null, error = null;
            try { got = ReadAs(field, json); }
            catch (JsonException ex) { error = ex.Message; }
            if (expected == null && error == null) Fail(id, "expected a refusal, got " + got);
            else if (expected == null && !error.Contains("[json-number-form]")) Fail(id, "refused without [json-number-form]: " + error);
            else if (expected != null && error != null) Fail(id, "refused: " + error);
            else if (expected != null && Normal(got) != Normal(expected)) Fail(id, "expected " + expected + ", got " + got);
        }
        Console.WriteLine("codec-vectors-json-numbers.json (System.Text.Json binding): " + (total - (_fails - failedBefore)) + "/" + total + " pass");
    }

    // An embedded value into an interface or abstract T is read through T's own converter, and refused when T has none.
    static void RunRefIntoInterface()
    {
        int failedBefore = _fails;
        string json = "{\"shape\": {\"$type\": \"" + ShapeJsonConverter.SquareType + "\", \"side\": 3},"
            + " \"shapes\": [{\"$ref\": \"probe.example.com/shapes@1.0.0/c1\"}, {\"radius\": 2}]}";
        var drawing = JsonSerializer.Deserialize<Drawing>(json);
        if (!(drawing.Shape.Value is Square square) || square.Side != 3)
            Fail("ref into interface: one", "read as " + drawing.Shape.Value?.GetType().Name);
        if (!drawing.Shapes[0].IsReference || drawing.Shapes[0].Uri != "probe.example.com/shapes@1.0.0/c1")
            Fail("ref into interface: a reference in a list", "read as " + drawing.Shapes[0].Uri);
        if (!(drawing.Shapes[1].Value is Circle circle) || circle.Radius != 2)
            Fail("ref into interface: an embedded value in a list", "read as " + drawing.Shapes[1].Value?.GetType().Name);
        var written = JsonDocument.Parse(JsonSerializer.Serialize(drawing)).RootElement;
        if (written.GetProperty("shape").GetProperty("side").GetInt64() != 3)
            Fail("ref into interface: written", "wrote " + written.GetRawText());
        foreach (var (id, sketch, named) in new[]
        {
            ("ref into an interface with no converter", "{\"thing\": {\"a\": 1}}", "IUnconverted"),
            ("ref into an abstract class with no converter", "{\"other\": {\"a\": 1}}", "AbstractUnconverted"),
        })
        {
            try { JsonSerializer.Deserialize<Sketch>(sketch); Fail(id, "expected a refusal"); }
            catch (JsonException e)
            {
                if (!e.Message.Contains(named) || !e.Message.Contains("[JsonConverter]")) Fail(id, "refused without naming the type and the way out: " + e.Message);
            }
        }
        Console.WriteLine("Ref<T> into an interface or abstract T: " + (6 - (_fails - failedBefore)) + "/6 pass");
    }

    static void Fail(string caseId, string message)
    {
        _fails++;
        Console.Error.WriteLine("FAIL  " + caseId + ": " + message);
    }

    static string FindVectorsDir()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir != null)
        {
            string candidate = Path.Combine(dir.FullName, "vectors", "codec-vectors-embedded.json");
            if (File.Exists(candidate)) return Path.Combine(dir.FullName, "vectors");
            candidate = Path.Combine(dir.FullName, "kanonak-codec", "vectors", "codec-vectors-embedded.json");
            if (File.Exists(candidate)) return Path.Combine(dir.FullName, "kanonak-codec", "vectors");
            dir = dir.Parent;
        }
        throw new FileNotFoundException("codec vectors directory not found walking up from " + AppContext.BaseDirectory);
    }
}
