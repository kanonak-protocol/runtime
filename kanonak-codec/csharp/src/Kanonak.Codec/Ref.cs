using System;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Kanonak.Codec
{
    /// <summary>
    /// An object property's value: EXACTLY ONE of a reference to a named
    /// resource (its canonical URI) or an embedded node (the value itself,
    /// carried inline — no independent identity). This is the typed twin of the
    /// wire form's <c>{"$ref": uri}</c> vs embedded-node distinction; the choice
    /// between the arms is authorial and hash-relevant, so it is explicit here,
    /// never inferred.
    /// </summary>
    [JsonConverter(typeof(RefJsonConverterFactory))]
    public sealed class Ref<T> where T : class
    {
        private Ref(string uri, T value)
        {
            Uri = uri;
            Value = value;
        }

        /// <summary>The referenced resource's canonical URI — the reference arm (else null).</summary>
        public string Uri { get; }

        /// <summary>The embedded value — the embedded arm (else null).</summary>
        public T Value { get; }

        /// <summary>True when this is the reference arm.</summary>
        public bool IsReference => Uri != null;

        /// <summary>A reference to a named resource by its canonical URI.</summary>
        public static Ref<T> To(string uri)
        {
            if (string.IsNullOrEmpty(uri))
                throw new ArgumentException("A reference needs a canonical URI.", nameof(uri));
            return new Ref<T>(uri, null);
        }

        /// <summary>
        /// A reference to a named resource by the instance itself — resolved
        /// through the target's <see cref="KanonakNode.Id"/>. The target must
        /// already carry its identity; an embedded (id-less) value cannot be
        /// referenced (embed it, or give it a name at the package level).
        /// </summary>
        public static Ref<T> To(T target)
        {
            if (target == null) throw new ArgumentNullException(nameof(target));
            var node = target as KanonakNode;
            if (node == null || string.IsNullOrEmpty(node.Id))
                throw new ArgumentException(
                    "Ref<T>.To(target) requires a KanonakNode with a non-empty Id — " +
                    "to carry the value inline instead, use Ref<T>.Embed(value).");
            return new Ref<T>(node.Id, null);
        }

        /// <summary>An embedded value, carried inline (derived identity, no $id).</summary>
        public static Ref<T> Embed(T value)
        {
            if (value == null) throw new ArgumentNullException(nameof(value));
            return new Ref<T>(null, value);
        }

        /// <summary>An embedded value with its authored dict-key name (hash-relevant).</summary>
        public static Ref<T> Embed(T value, string name)
        {
            if (value == null) throw new ArgumentNullException(nameof(value));
            var node = value as KanonakNode;
            if (node == null)
                throw new ArgumentException(
                    "Naming an embedded value requires a KanonakNode (the name rides $name).");
            node.Name = name;
            return new Ref<T>(null, value);
        }
    }

    /// <summary>
    /// (De)serializes <see cref="Ref{T}"/>: the reference arm as
    /// <c>{"$ref": uri}</c>, the embedded arm as the value's own wire form —
    /// serialized by its RUNTIME type, so an interface-typed <c>T</c> still
    /// carries the concrete class's full contract. Reading the embedded arm
    /// requires a concrete <c>T</c> (deserializing into an interface needs a
    /// type registry — fail-loud until that exists).
    /// </summary>
    public sealed class RefJsonConverterFactory : JsonConverterFactory
    {
        public override bool CanConvert(Type typeToConvert)
            => typeToConvert.IsGenericType && typeToConvert.GetGenericTypeDefinition() == typeof(Ref<>);

        public override JsonConverter CreateConverter(Type typeToConvert, JsonSerializerOptions options)
        {
            var valueType = typeToConvert.GetGenericArguments()[0];
            var converterType = typeof(RefJsonConverter<>).MakeGenericType(valueType);
            return (JsonConverter)Activator.CreateInstance(converterType);
        }

        private sealed class RefJsonConverter<T> : JsonConverter<Ref<T>> where T : class
        {
            public override void Write(Utf8JsonWriter writer, Ref<T> value, JsonSerializerOptions options)
            {
                if (value.IsReference)
                {
                    writer.WriteStartObject();
                    writer.WriteString("$ref", value.Uri);
                    writer.WriteEndObject();
                    return;
                }
                // The runtime type, not T — an interface-typed T would otherwise
                // serialize only the interface's declared members.
                JsonSerializer.Serialize(writer, (object)value.Value, value.Value.GetType(), options);
            }

            public override Ref<T> Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
            {
                using (var doc = JsonDocument.ParseValue(ref reader))
                {
                    if (doc.RootElement.ValueKind == JsonValueKind.Object &&
                        doc.RootElement.TryGetProperty("$ref", out var uri))
                    {
                        return Ref<T>.To(uri.GetString());
                    }
                    if (typeof(T).IsInterface || typeof(T).IsAbstract)
                    {
                        throw new JsonException(
                            "Cannot deserialize an embedded value into the interface/abstract type " +
                            typeof(T).Name + " — a concrete type registry keyed by $type is required " +
                            "and not yet part of the codec runtime.");
                    }
                    var value = (T)JsonSerializer.Deserialize(doc.RootElement.GetRawText(), typeof(T), options);
                    return Ref<T>.Embed(value);
                }
            }
        }
    }
}
