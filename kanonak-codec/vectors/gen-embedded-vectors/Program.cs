// Generates codec-vectors-embedded.json: the kanonak-codec 0.2.0 conformance
// cases for embedded values in the node contract. Expected canonical forms and
// hashes are computed by Kanonak.Canonical 0.1.0, whose Embedded serialization
// is itself pinned by the published ref-and-embedded canonical vector — so the
// expected values here are derived from the frozen canonical layer, not typed
// by hand. Structural semantics (list wrapping, name presence, explicit-type
// statements) mirror the reference implementation (@kanonak-protocol/cli).
using System.Text.Json;
using System.Text.Json.Nodes;
using Kanonak.Canonical;

const string SCHEMA = "probe.example.com/schema@1.0.0";
const string DATA = "probe.example.com/data@1.0.0";
const string TYPE_PRED = "kanonak.org/core-rdf@1.1.0/type";
const string LABEL_PRED = "kanonak.org/core-rdf@1.1.0/label";
const string PKG_TYPE = "kanonak.org/core-kanonak@1.0.0/Package";
const string XSD = "kanonak.org/core-xsd";

string outPath = args[0];

// -- canonical model helpers -------------------------------------------------

Statement TypeStmt(string typeUri) => new(TYPE_PRED, new Reference(typeUri));
Statement Str(string localName, string v) => new($"{SCHEMA}/{localName}", new TypedScalar(Carrier.String, v));
Statement Int(string localName, string v) => new($"{SCHEMA}/{localName}", new TypedScalar(Carrier.Integer, v));

Subject PackageSubject(string label) => new($"{DATA}/data", new List<Statement>
{
    new(LABEL_PRED, new RawScalar(label)),
    TypeStmt(PKG_TYPE),
});

// -- vector schema block (0.2.0: object props carry `range` for embedded field mapping)

JsonObject Prop(string localName, string kind, string? datatype = null, string? range = null)
{
    var o = new JsonObject { ["predicate"] = $"{SCHEMA}/{localName}", ["kind"] = kind };
    if (datatype != null) o["datatype"] = datatype;
    if (range != null) o["range"] = range;
    return o;
}

var schemaBlock = new JsonObject
{
    ["typePredicate"] = TYPE_PRED,
    ["labelPredicate"] = LABEL_PRED,
    ["packageTypeUri"] = PKG_TYPE,
    ["classes"] = new JsonObject
    {
        [$"{SCHEMA}/Order"] = new JsonObject
        {
            ["typeUri"] = $"{SCHEMA}/Order",
            ["props"] = new JsonObject
            {
                ["note"] = Prop("note", "datatype", $"{XSD}/string"),
                ["items"] = Prop("items", "object", range: $"{SCHEMA}/LineItem"),
                ["customer"] = Prop("customer", "object", range: $"{SCHEMA}/Customer"),
            },
        },
        [$"{SCHEMA}/LineItem"] = new JsonObject
        {
            ["typeUri"] = $"{SCHEMA}/LineItem",
            ["props"] = new JsonObject
            {
                ["sku"] = Prop("sku", "datatype", $"{XSD}/string"),
                ["qty"] = Prop("qty", "datatype", $"{XSD}/integer"),
            },
        },
        [$"{SCHEMA}/Customer"] = new JsonObject
        {
            ["typeUri"] = $"{SCHEMA}/Customer",
            ["props"] = new JsonObject
            {
                ["name"] = Prop("name", "datatype", $"{XSD}/string"),
                ["address"] = Prop("address", "object", range: $"{SCHEMA}/Address"),
            },
        },
        [$"{SCHEMA}/Address"] = new JsonObject
        {
            ["typeUri"] = $"{SCHEMA}/Address",
            ["props"] = new JsonObject
            {
                ["city"] = Prop("city", "datatype", $"{XSD}/string"),
            },
        },
    },
};

// -- node builders (wire form) -----------------------------------------------

JsonObject LineItemNode(string? name, string sku, int qty, bool explicitType = false)
{
    var n = new JsonObject();
    if (name != null) n["$name"] = name;
    if (explicitType) n["$type"] = $"{SCHEMA}/LineItem";
    n["sku"] = sku;
    n["qty"] = qty;
    return n;
}

JsonObject OrderNode(string id, JsonNode? items = null, JsonNode? customer = null, string? note = null)
{
    var n = new JsonObject { ["$type"] = $"{SCHEMA}/Order", ["$id"] = $"{DATA}/{id}" };
    if (note != null) n["note"] = note;
    if (items != null) n["items"] = items;
    if (customer != null) n["customer"] = customer;
    return n;
}

Embedded LineItemEmbed(string? name, string sku, int qty, bool explicitType = false)
{
    var stmts = new List<Statement> { Str("sku", sku), Int("qty", qty.ToString()) };
    if (explicitType) stmts.Add(TypeStmt($"{SCHEMA}/LineItem"));
    return new Embedded(name!, stmts);
}

// -- cases ---------------------------------------------------------------------

var cases = new JsonArray();
var pkgBlock = new JsonObject
{
    ["publisher"] = "probe.example.com",
    ["packageName"] = "data",
    ["version"] = "1.0.0",
    ["label"] = "Embedded Probe Data",
};

void AddCase(string id, string description, JsonArray nodes, Package canonical)
{
    cases.Add(new JsonObject
    {
        ["id"] = id,
        ["description"] = description,
        ["pkg"] = pkgBlock.DeepClone(),
        ["nodes"] = nodes,
        ["expectedCanonicalForm"] = CanonicalForm.Serialize(canonical),
        ["expectedHash"] = CanonicalForm.Hash(canonical),
        ["expectedSerialize"] = nodes.DeepClone(),
    });
}

Package Pkg(params Subject[] subjects)
{
    var list = new List<Subject>(subjects) { PackageSubject("Embedded Probe Data") };
    return new Package(list);
}

// 1. embedded-named-in-list
AddCase(
    "embedded-named-in-list",
    "A named embedded value ($name = the authored dict-key) inside a list-valued object property. " +
    "The embedded carries NO type statement — its type is inferred from the property's range and " +
    "is not materialized in canonical form.",
    new JsonArray(OrderNode("o1", items: new JsonArray(LineItemNode("first", "X", 1)), note: "A")),
    Pkg(new Subject($"{DATA}/o1", new List<Statement>
    {
        TypeStmt($"{SCHEMA}/Order"),
        Str("note", "A"),
        new($"{SCHEMA}/items", new KList(new List<CanonicalValue> { LineItemEmbed("first", "X", 1) })),
    })));

// 2. embedded-unnamed-positional
AddCase(
    "embedded-unnamed-positional",
    "The same embedded content with NO $name (a positional list item). The name participates in " +
    "content identity, so this case hashes differently from embedded-named-in-list.",
    new JsonArray(OrderNode("o1", items: new JsonArray(LineItemNode(null, "X", 1)), note: "A")),
    Pkg(new Subject($"{DATA}/o1", new List<Statement>
    {
        TypeStmt($"{SCHEMA}/Order"),
        Str("note", "A"),
        new($"{SCHEMA}/items", new KList(new List<CanonicalValue> { LineItemEmbed(null, "X", 1) })),
    })));

// 3. embedded-explicit-type
AddCase(
    "embedded-explicit-type",
    "An embedded value carrying an explicit $type emits a type statement inside the embedded — " +
    "hash-relevant even when it equals the range-derived type (mirrors the reference " +
    "implementation, which warns that the declaration is redundant but still canonicalizes it).",
    new JsonArray(OrderNode("o1", items: new JsonArray(LineItemNode("first", "X", 1, explicitType: true)), note: "A")),
    Pkg(new Subject($"{DATA}/o1", new List<Statement>
    {
        TypeStmt($"{SCHEMA}/Order"),
        Str("note", "A"),
        new($"{SCHEMA}/items", new KList(new List<CanonicalValue> { LineItemEmbed("first", "X", 1, explicitType: true) })),
    })));

// 4 + 5. list order pair
AddCase(
    "embedded-list-order",
    "Two named embeddeds in source order [a, b]. List order is fully semantic and hashed.",
    new JsonArray(OrderNode("o1", items: new JsonArray(LineItemNode("a", "X", 1), LineItemNode("b", "Y", 2)))),
    Pkg(new Subject($"{DATA}/o1", new List<Statement>
    {
        TypeStmt($"{SCHEMA}/Order"),
        new($"{SCHEMA}/items", new KList(new List<CanonicalValue>
        {
            LineItemEmbed("a", "X", 1), LineItemEmbed("b", "Y", 2),
        })),
    })));

AddCase(
    "embedded-list-order-swapped",
    "The SAME two embeddeds in swapped order [b, a] — a deliberately different expected canonical " +
    "form and hash from embedded-list-order. A port that normalizes, sorts, or randomizes embedded " +
    "collection order (e.g. by representing the dict-keyed group as a map) fails this pair.",
    new JsonArray(OrderNode("o1", items: new JsonArray(LineItemNode("b", "Y", 2), LineItemNode("a", "X", 1)))),
    Pkg(new Subject($"{DATA}/o1", new List<Statement>
    {
        TypeStmt($"{SCHEMA}/Order"),
        new($"{SCHEMA}/items", new KList(new List<CanonicalValue>
        {
            LineItemEmbed("b", "Y", 2), LineItemEmbed("a", "X", 1),
        })),
    })));

// 6. embedded-nested
AddCase(
    "embedded-nested",
    "An embedded inside an embedded (customer -> address), each level named. Field mapping at " +
    "every depth uses the containing property's range class.",
    new JsonArray(OrderNode("o1", customer: new JsonArray(new JsonObject
    {
        ["$name"] = "cust",
        ["name"] = "Ada",
        ["address"] = new JsonArray(new JsonObject { ["$name"] = "home", ["city"] = "Austin" }),
    }))),
    Pkg(new Subject($"{DATA}/o1", new List<Statement>
    {
        TypeStmt($"{SCHEMA}/Order"),
        new($"{SCHEMA}/customer", new KList(new List<CanonicalValue>
        {
            new Embedded("cust", new List<Statement>
            {
                Str("name", "Ada"),
                new($"{SCHEMA}/address", new KList(new List<CanonicalValue>
                {
                    new Embedded("home", new List<Statement> { Str("city", "Austin") }),
                })),
            }),
        })),
    })));

// 7. single-embedded-bare
AddCase(
    "single-embedded-bare",
    "A bare map (no $ref, not in a list) under an object property is a single embedded value — " +
    "canonicalized as a bare embedded, not a one-element list. Wire list-ness is explicit: the " +
    "authored dict-keyed YAML form always arrives from the parser as a list; a node built " +
    "programmatically may carry a bare embedded.",
    new JsonArray(OrderNode("o1", customer: new JsonObject { ["$name"] = "cust", ["name"] = "Ada" })),
    Pkg(new Subject($"{DATA}/o1", new List<Statement>
    {
        TypeStmt($"{SCHEMA}/Order"),
        new($"{SCHEMA}/customer", new Embedded("cust", new List<Statement> { Str("name", "Ada") })),
    })));

// 8. empty-list-emits-nothing
AddCase(
    "empty-list-emits-nothing",
    "An empty list under any property contributes NO statement to the canonical form (absent and " +
    "empty are identical at the canonical layer). The wire serialize preserves the empty list.",
    new JsonArray(OrderNode("o1", items: new JsonArray(), note: "A")),
    Pkg(new Subject($"{DATA}/o1", new List<Statement>
    {
        TypeStmt($"{SCHEMA}/Order"),
        Str("note", "A"),
    })));

// -- emit ----------------------------------------------------------------------

var root = new JsonObject
{
    ["description"] =
        "Codec conformance vectors for EMBEDDED VALUES in the node contract — gates kanonak-codec " +
        "0.2.0 (see kanonak-protocol/runtime#1). Ports implementing the 0.1.0 contract do not run " +
        "this file. Wire shape: an embedded value is a map with no $id, an optional $name (the " +
        "authored dict-key; hash-relevant), an optional $type (emits a type statement when present), " +
        "and schema-mapped fields; field mapping without $type uses the containing object property's " +
        "`range` (the 0.2.0 schema addition). Expected values are authoritative; canonical " +
        "serialization of embedded values is additionally pinned by the ref-and-embedded case in " +
        "kanonak-canonical/vectors.",
    ["canonicalFormVersion"] = "1",
    ["schema"] = schemaBlock,
    ["cases"] = cases,
};

var opts = new JsonSerializerOptions { WriteIndented = true, Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
File.WriteAllText(outPath, root.ToJsonString(opts) + "\n");
Console.WriteLine($"wrote {outPath}");
foreach (var c in cases)
    Console.WriteLine($"  {c!["id"]}: {c["expectedHash"]}");
