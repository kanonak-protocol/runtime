using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace Kanonak.Codec
{
    /// <summary>
    /// The <c>$</c>-envelope as data — the base class a generated typed model
    /// extends so an instance carries its own identity and serializes straight
    /// to the normalized-JSON wire form via System.Text.Json. Envelope keys are
    /// reserved (never ontology statements); <see cref="Extra"/> holds the
    /// open-world assertions outside the type-model, keyed by predicate URI,
    /// and rides the wire as sibling fields (<c>[JsonExtensionData]</c>
    /// semantics — matching <see cref="Codec.Serialize"/>).
    /// </summary>
    public abstract class KanonakNode
    {
        /// <summary>The resource's canonical URI. Required to form a subject.</summary>
        [JsonPropertyName("$id")]
        [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
        public string Id { get; set; }

        /// <summary>The durable class URI — the value of the synthesized type triple.</summary>
        [JsonPropertyName("$type")]
        [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
        public string Type { get; set; }

        /// <summary>
        /// A multi-typed node's FULL type set (0.4.0, runtime#10) — present only
        /// when the node carries more than one type statement. Sorted by UTF-8
        /// bytes, at least two members, no duplicates, <see cref="Type"/> a
        /// member; each member emits one type statement in canonical form.
        /// Exposed ONLY as the <c>$types</c> envelope — deliberately no
        /// unprefixed wire name, because an ontology can model a property
        /// literally named <c>types</c>; the <c>$</c> prefix exists to avoid
        /// exactly that collision.
        /// </summary>
        [JsonPropertyName("$types")]
        [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
        public List<string> Types { get; set; }

        /// <summary>
        /// An embedded value's authored dict-key — HASH-RELEVANT (serialized into
        /// the canonical form). Only meaningful when this instance is used as an
        /// embedded value (via <see cref="Ref{T}.Embed(T)"/>); null for subjects.
        /// </summary>
        [JsonPropertyName("$name")]
        [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
        public string Name { get; set; }

        /// <summary>Package provenance on read; ignored if echoed back on write.</summary>
        [JsonPropertyName("$contentHash")]
        [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
        public string PackageContentHash { get; set; }

        /// <summary>Package provenance on read; ignored if echoed back on write.</summary>
        [JsonPropertyName("$version")]
        [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
        public string PackageVersion { get; set; }

        /// <summary>
        /// Open-world assertions outside the type-model, keyed by predicate URI.
        /// Lossless round-trip: a consumer that ignores this still carries it.
        /// </summary>
        [JsonExtensionData]
        public Dictionary<string, object> Extra { get; set; }
    }
}
