using System;
using System.Collections.Generic;
using System.Text.Json;

namespace Kanonak.Codec
{
    /// <summary>
    /// The typed-model binding: a <see cref="KanonakNode"/>-based object graph
    /// → the codec's node contract → canonical form / content hash. The bridge
    /// is native serde, not reflection: the typed object serializes to its
    /// normalized-JSON wire form via System.Text.Json (envelope-as-data +
    /// <see cref="Ref{T}"/> values), and the wire form maps onto the node
    /// contract through the SAME split <see cref="Codec.Deserialize"/> defines —
    /// so the typed path and the dictionary path are one contract, not two.
    /// </summary>
    public static class TypedNodes
    {
        private static readonly JsonSerializerOptions WireOptions = new JsonSerializerOptions
        {
            // Absence is the data model's optionality — nulls never ride the wire.
            DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull,
        };

        /// <summary>A typed instance's codec node (the dictionary contract).</summary>
        public static Dictionary<string, object> ToNode(KanonakNode typed, CodecSchema schema)
        {
            if (typed == null) throw new ArgumentNullException(nameof(typed));
            string wire = JsonSerializer.Serialize(typed, typed.GetType(), WireOptions);
            using (var doc = JsonDocument.Parse(wire))
            {
                var json = (Dictionary<string, object>)FromElement(doc.RootElement);
                // The wire form carries open-world extras as sibling fields;
                // Deserialize splits them back under $extra per the node contract.
                return Codec.Deserialize(json, schema);
            }
        }

        /// <summary>The canonical form of a package built from typed instances.</summary>
        public static string CanonicalForm(IEnumerable<KanonakNode> typed, CodecSchema schema, PackageContext pkg)
            => Codec.CanonicalForm(ToNodes(typed, schema), schema, pkg);

        /// <summary>The <c>sha256:</c> content hash of a package built from typed instances.</summary>
        public static string ContentHash(IEnumerable<KanonakNode> typed, CodecSchema schema, PackageContext pkg)
            => Codec.ContentHash(ToNodes(typed, schema), schema, pkg);

        private static List<IReadOnlyDictionary<string, object>> ToNodes(
            IEnumerable<KanonakNode> typed, CodecSchema schema)
        {
            if (typed == null) throw new ArgumentNullException(nameof(typed));
            var nodes = new List<IReadOnlyDictionary<string, object>>();
            foreach (var t in typed) nodes.Add(ToNode(t, schema));
            return nodes;
        }

        /// <summary>
        /// A JSON element as the node contract's CLR shapes: string/bool,
        /// long-else-decimal numbers (the codec's lexical entry re-normalizes),
        /// <c>Dictionary&lt;string, object&gt;</c> maps, <c>List&lt;object&gt;</c> lists.
        /// </summary>
        private static object FromElement(JsonElement e)
        {
            switch (e.ValueKind)
            {
                case JsonValueKind.String: return e.GetString();
                case JsonValueKind.True: return true;
                case JsonValueKind.False: return false;
                case JsonValueKind.Number:
                    long l;
                    if (e.TryGetInt64(out l)) return l;
                    return e.GetDecimal();
                case JsonValueKind.Object:
                    var map = new Dictionary<string, object>();
                    foreach (var p in e.EnumerateObject()) map[p.Name] = FromElement(p.Value);
                    return map;
                case JsonValueKind.Array:
                    var list = new List<object>();
                    foreach (var item in e.EnumerateArray()) list.Add(FromElement(item));
                    return list;
                case JsonValueKind.Null: return null;
                default:
                    throw new ArgumentException("Unsupported JSON value kind: " + e.ValueKind);
            }
        }
    }
}
