using System.Collections.Generic;
using System.Text.Json;

namespace Kanonak.Codec
{
    /// <summary>One property's canonicalization metadata, as embedded by the generator.</summary>
    public sealed class CodecProp
    {
        /// <summary>The predicate's durable canonical URI (with resolved version).</summary>
        public string Predicate;

        /// <summary><c>"datatype"</c> vs <c>"object"</c> — decides typed-scalar vs reference/embedded.</summary>
        public string Kind;

        /// <summary>The datatype's canonical URI (carrier source) — present for datatype props.</summary>
        public string Datatype;

        /// <summary>
        /// The range class's canonical URI — present for object props (0.2.0). Maps an
        /// embedded value's fields when the embedded carries no explicit <c>$type</c>
        /// (range-derived typing: inference only, never materialized as a statement).
        /// </summary>
        public string Range;
    }

    /// <summary>A class's canonicalization schema: its durable URI + its (flattened) props.</summary>
    public sealed class CodecClass
    {
        /// <summary>The class's durable canonical URI — the value of the synthesized type triple.</summary>
        public string TypeUri;

        /// <summary>Properties keyed by local name (the wire field name).</summary>
        public Dictionary<string, CodecProp> Props = new Dictionary<string, CodecProp>();
    }

    /// <summary>
    /// The metadata a generated SDK embeds for its codec — the resolved foundation
    /// URIs plus the SDK's own classes, keyed by their durable <c>$type</c> URIs. It
    /// carries no package identity: the instances a consumer builds live in the
    /// consumer's own (data) package, whose identity is supplied at call time via
    /// <see cref="PackageContext"/>.
    /// </summary>
    /// <summary>
    /// One member of a closed set of named individuals. A class even when
    /// <c>Label</c> is its only field, so a later addition (an ordered scale's
    /// dominates closure, runtime#16) stays additive rather than a breaking
    /// shape change.
    /// </summary>
    public sealed class CodecEnumMember
    {
        /// <summary>
        /// The member's own label — the ontology's, read through the same
        /// predicate <c>LabelPredicate</c> names. Display-lens resolution is a
        /// RENDERING concern and deliberately not here.
        /// </summary>
        public string Label;
    }

    /// <summary>
    /// A closed set of named individuals — the metadata that lets a consumer
    /// holding only the schema resolve a member URI it received on the wire,
    /// with no generated SDK for the declaring package.
    /// <para>
    /// Carried, never enforced. A prop ranged at an enumeration in ANOTHER
    /// package has no entry here (Classes and Enums both describe the SDK's OWN
    /// package), so a codec that validated a <c>$ref</c> against these members
    /// would false-reject valid cross-package data. Absence means "not mine",
    /// never "invalid".
    /// </para>
    /// <para>
    /// What makes a set closed is the PRODUCER's decision, taken in the
    /// ontology and resolved by the schema layer above; this runtime carries
    /// the outcome and names no vocabulary term.
    /// </para>
    /// </summary>
    public sealed class CodecEnum
    {
        /// <summary>The enumeration class's durable URI — the same key form Classes uses.</summary>
        public string TypeUri;

        /// <summary>
        /// Members keyed by durable VERSIONED URI
        /// (<c>publisher/package@version/name</c>), byte-identical to what the
        /// wire carries as <c>{"$ref": ...}</c>. A versionless key would look
        /// right and miss every lookup, so the formation is an invariant, not a
        /// convention. The local name is NOT duplicated as a key: derive it from
        /// the URI with canonical's local-name parser (runtime#17).
        /// </summary>
        public Dictionary<string, CodecEnumMember> Members = new Dictionary<string, CodecEnumMember>();
    }

    public sealed class CodecSchema
    {
        /// <summary>Resolved <c>kanonak.org/core-rdf@&lt;ver&gt;/type</c> predicate URI.</summary>
        public string TypePredicate;

        /// <summary>Resolved <c>kanonak.org/core-rdf@&lt;ver&gt;/label</c> predicate URI.</summary>
        public string LabelPredicate;

        /// <summary>Resolved <c>kanonak.org/core-kanonak@&lt;ver&gt;/Package</c> class URI.</summary>
        public string PackageTypeUri;

        /// <summary>Classes keyed by durable type URI (the node's <c>$type</c>).</summary>
        public Dictionary<string, CodecClass> Classes = new Dictionary<string, CodecClass>();

        /// <summary>
        /// Closed sets of named individuals keyed by durable type URI (0.5.0,
        /// runtime#21). Optional, so it is additive. Canonicalization-INERT —
        /// content hashes are unchanged by its presence, which the enums
        /// vectors pin rather than assert. An entry here does NOT require a
        /// twin in <see cref="Classes"/>: a Classes entry is needed only when
        /// instances of that class are emitted as NODES, which an
        /// enumeration's members are not. That absence is load-bearing — it is
        /// what makes an embedded value on an enum-ranged property fail loudly
        /// instead of being mapped as if it were a resource.
        /// </summary>
        public Dictionary<string, CodecEnum> Enums = new Dictionary<string, CodecEnum>();

        /// <summary>
        /// Parse the embedded schema JSON (the form the generators emit and the
        /// shared vectors carry) into a <see cref="CodecSchema"/>. Generated SDKs
        /// embed their schema as a JSON string and call this once at load. Kept
        /// minimal — it is the single public deserializer the generated codec binds to.
        /// </summary>
        public static CodecSchema FromJson(string json)
        {
            using var doc = JsonDocument.Parse(json);
            JsonElement s = doc.RootElement;
            var schema = new CodecSchema
            {
                TypePredicate = s.GetProperty("typePredicate").GetString(),
                LabelPredicate = s.GetProperty("labelPredicate").GetString(),
                PackageTypeUri = s.GetProperty("packageTypeUri").GetString(),
            };
            foreach (var cls in s.GetProperty("classes").EnumerateObject())
            {
                var cc = new CodecClass { TypeUri = cls.Value.GetProperty("typeUri").GetString() };
                foreach (var p in cls.Value.GetProperty("props").EnumerateObject())
                {
                    var prop = new CodecProp
                    {
                        Predicate = p.Value.GetProperty("predicate").GetString(),
                        Kind = p.Value.GetProperty("kind").GetString(),
                    };
                    if (p.Value.TryGetProperty("datatype", out var dt)) prop.Datatype = dt.GetString();
                    if (p.Value.TryGetProperty("range", out var rg)) prop.Range = rg.GetString();
                    cc.Props[p.Name] = prop;
                }
                schema.Classes[cls.Name] = cc;
            }
            // Closed sets (0.5.0, runtime#21). Optional: a schema written
            // before this field remains valid and yields an empty map.
            if (s.TryGetProperty("enums", out var enums))
            {
                foreach (var en in enums.EnumerateObject())
                {
                    var ce = new CodecEnum { TypeUri = en.Value.GetProperty("typeUri").GetString() };
                    if (en.Value.TryGetProperty("members", out var members))
                    {
                        foreach (var m in members.EnumerateObject())
                        {
                            var mem = new CodecEnumMember();
                            if (m.Value.TryGetProperty("label", out var lbl)) mem.Label = lbl.GetString();
                            ce.Members[m.Name] = mem;
                        }
                    }
                    schema.Enums[en.Name] = ce;
                }
            }
            return schema;
        }
    }

    /// <summary>
    /// The identity of the (data) package being content-addressed — the consumer's
    /// package the nodes are assembled into. Used to synthesize the package-wrapper
    /// subject <c>&lt;publisher&gt;/&lt;packageName&gt;@&lt;version&gt;/&lt;packageName&gt;</c>.
    /// </summary>
    public sealed class PackageContext
    {
        public string Publisher;
        public string PackageName;
        public string Version;

        /// <summary>Optional package label (a raw/untyped string statement, as the parser emits).</summary>
        public string Label;
    }
}
