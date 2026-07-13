// Typed-surface conformance: hand-written GENERATED-STYLE classes for the
// embedded-vectors probe schema, driven through the KanonakNode / Ref<T> /
// TypedNodes binding, asserted against the SAME golden vectors the node
// contract is gated by (codec-vectors.json + codec-vectors-embedded.json).
// This file is also the executable spec for what an SDK generator must emit:
// classes extend KanonakNode, object properties are Ref<T>/List<Ref<T>>,
// wire names ride [JsonPropertyName].
using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;
using Kanonak.Codec;

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
