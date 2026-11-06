package org.kanonak.codec.jackson;

import com.fasterxml.jackson.core.JsonGenerator;
import com.fasterxml.jackson.core.JsonParser;
import com.fasterxml.jackson.databind.BeanProperty;
import com.fasterxml.jackson.databind.DeserializationContext;
import com.fasterxml.jackson.databind.JavaType;
import com.fasterxml.jackson.databind.JsonDeserializer;
import com.fasterxml.jackson.databind.JsonMappingException;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.SerializerProvider;
import com.fasterxml.jackson.databind.deser.ContextualDeserializer;
import com.fasterxml.jackson.databind.deser.std.StdDeserializer;
import com.fasterxml.jackson.databind.ser.std.StdSerializer;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import org.kanonak.codec.KanonakNode;
import org.kanonak.codec.Ref;

/**
 * The Jackson binding of {@link Ref}, an object property's value (runtime#68), in the wire form the codec's typed path
 * writes: a reference as {@code {"$ref": uri}}, and an embedded node in place, read as the field's own {@code T}. A
 * {@code $ref} with anything beside it, an embedded node carrying {@code $id}, and a value that isn't a JSON object
 * are refused, each with a {@link JsonMappingException} naming what was found.
 */
final class RefBinding {
    private RefBinding() {
    }

    private static final String EMBEDDED_ID = "An embedded value must not carry an id (derived identity, no $id): "
        + "a named resource is referenced as {\"$ref\": uri}, with Ref.to(...).";

    /** Writes a {@link Ref}: a reference as {@code {"$ref": uri}}, an embedded value as Jackson writes its class. */
    static final class RefSerializer extends StdSerializer<Ref<?>> {
        private static final long serialVersionUID = 1L;

        RefSerializer() {
            super(Ref.class, false);
        }

        @Override
        public void serialize(Ref<?> ref, JsonGenerator gen, SerializerProvider provider) throws IOException {
            if (ref.isReference()) {
                gen.writeStartObject();
                gen.writeStringField("$ref", ref.getUri());
                gen.writeEndObject();
                return;
            }
            Object embedded = ref.getValue();
            if (embedded instanceof KanonakNode node && node.getId() != null && !node.getId().isEmpty()) {
                throw JsonMappingException.from(gen, EMBEDDED_ID);
            }
            provider.defaultSerializeValue(embedded, gen);
        }
    }

    /** Reads a {@link Ref} of the field's own {@code T}, which only the field's declared type can give. */
    static final class RefDeserializer extends StdDeserializer<Ref<?>> implements ContextualDeserializer {
        private static final long serialVersionUID = 1L;
        private final JavaType valueType;

        RefDeserializer() {
            this(null);
        }

        private RefDeserializer(JavaType valueType) {
            super(Ref.class);
            this.valueType = valueType;
        }

        @Override
        public JsonDeserializer<?> createContextual(DeserializationContext ctxt, BeanProperty property)
            throws JsonMappingException {
            JavaType declared = ctxt.getContextualType() != null ? ctxt.getContextualType()
                : property != null ? property.getType() : null;
            JavaType value = declared == null ? null : declared.containedType(0);
            if (value == null) {
                throw JsonMappingException.from(ctxt, "A Ref is read only as a declared Ref<T>, which says what an "
                    + "embedded value is; found " + (declared == null ? "no declared type" : declared.toString())
                    + (property != null ? " on " + property.getName() : "") + ".");
            }
            return new RefDeserializer(value);
        }

        @Override
        public Ref<?> deserialize(JsonParser p, DeserializationContext ctxt) throws IOException {
            if (valueType == null) {
                throw JsonMappingException.from(p, "A Ref is read only as a declared Ref<T>, which says what an "
                    + "embedded value is.");
            }
            JsonNode tree = ctxt.readTree(p);
            if (!tree.isObject()) {
                throw JsonMappingException.from(p, "A Ref is {\"$ref\": uri} or a node embedded in place, never a JSON "
                    + tree.getNodeType().name().toLowerCase() + ": found " + tree + ".");
            }
            JsonNode uri = tree.get("$ref");
            if (uri != null) {
                if (tree.size() != 1) {
                    List<String> beside = new ArrayList<>();
                    tree.fieldNames().forEachRemaining(key -> {
                        if (!key.equals("$ref")) beside.add(key);
                    });
                    throw JsonMappingException.from(p, "A reference is {\"$ref\": uri} alone, with nothing beside it: "
                        + "found " + beside + " beside $ref.");
                }
                if (!uri.isTextual() || uri.asText().isEmpty()) {
                    throw JsonMappingException.from(p, "A reference's $ref is its canonical URI, a non-empty string: "
                        + "found " + uri + ".");
                }
                return Ref.to(uri.asText());
            }
            if (tree.has("$id")) {
                throw JsonMappingException.from(p, EMBEDDED_ID + " Found $id " + tree.get("$id") + ".");
            }
            return Ref.embed(ctxt.readTreeAsValue(tree, valueType));
        }
    }
}
