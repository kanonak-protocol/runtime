package org.kanonak.codec;

import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * The typed-model binding: a {@link KanonakNode}-based object graph → the
 * codec's node contract → canonical form / content hash. The bridge is a
 * reflective walk (the JDK-only twin of the C# serde path): the typed instance
 * projects to its normalized-JSON wire form (envelope-as-data + {@link Ref}
 * values + {@link WireName}-annotated fields), and the wire form maps onto the
 * node contract through the SAME split {@link Codec#deserialize} defines — so
 * the typed path and the dictionary path are one contract, not two.
 *
 * <p>The reference-vs-embedded choice is authorial and hash-relevant (never
 * inferred); an embedded value's {@code $name} is hash-relevant; range-derived
 * typing of an untyped embedded is inference only — no type statement is
 * emitted for it at the canonical layer.
 */
public final class TypedNodes {
    private TypedNodes() {}

    /**
     * A typed instance's codec node (the dictionary contract). The instance
     * projects to its wire map — envelope entries when non-null/non-empty,
     * {@link WireName}-annotated fields (nulls dropped: absence is the data
     * model's optionality), then {@link KanonakNode#getExtra() extra} entries
     * as sibling fields after the modeled ones (a modeled field wins a name
     * collision — {@link Codec#serialize} semantics) — and the wire map splits
     * back onto the node contract via {@link Codec#deserialize}.
     */
    public static Map<String, Object> toNode(Object typed, CodecSchema schema) {
        Objects.requireNonNull(typed, "typed");
        return Codec.deserialize(toWireMap(typed), schema);
    }

    /** A typed instance's normalized-JSON wire form as a map. */
    private static Map<String, Object> toWireMap(Object typed) {
        Map<String, Object> wire = new LinkedHashMap<>();
        if (typed instanceof KanonakNode node) {
            putEnvelope(wire, "$id", node.getId());
            putEnvelope(wire, "$type", node.getTypeUri());
            if (node.getTypes() != null && !node.getTypes().isEmpty()) {
                wire.put("$types", new ArrayList<>(node.getTypes()));
            }
            putEnvelope(wire, "$name", node.getName());
            putEnvelope(wire, "$contentHash", node.getPackageContentHash());
            putEnvelope(wire, "$version", node.getPackageVersion());
        }
        // The class hierarchy's @WireName-annotated fields. Walk order
        // (subclass-first) is irrelevant: the canonical form sorts.
        for (Class<?> c = typed.getClass(); c != null && c != Object.class; c = c.getSuperclass()) {
            for (Field field : c.getDeclaredFields()) {
                WireName wireName = field.getAnnotation(WireName.class);
                if (wireName == null) {
                    continue;
                }
                field.setAccessible(true);
                Object raw;
                try {
                    raw = field.get(typed);
                } catch (IllegalAccessException e) {
                    throw new IllegalStateException(
                        "Cannot read @WireName field " + c.getName() + "." + field.getName(), e);
                }
                if (raw == null) {
                    continue;
                }
                wire.put(wireName.value(), wireValue(raw));
            }
        }
        // Extras ride as sibling fields AFTER the modeled ones; a modeled
        // field wins a name collision (Codec.serialize semantics).
        if (typed instanceof KanonakNode node && node.getExtra() != null) {
            for (Map.Entry<String, Object> e : node.getExtra().entrySet()) {
                if (e.getValue() == null || wire.containsKey(e.getKey())) {
                    continue;
                }
                wire.put(e.getKey(), wireValue(e.getValue()));
            }
        }
        return wire;
    }

    /**
     * A typed field value as the wire form's shapes: scalars as-is, the
     * {@link Ref} arms as {@code {"$ref": uri}} vs the embedded value's own
     * wire map, lists item-by-item (an empty list STAYS an empty list — the
     * canonical layer drops it), nested nodes/maps recursively. Anything else
     * fails loud — no silent fallbacks.
     */
    private static Object wireValue(Object raw) {
        if (raw instanceof String || raw instanceof Boolean
            || raw instanceof Number || raw instanceof JsonNumber) {
            return raw;
        }
        if (raw instanceof Ref<?> ref) {
            if (ref.isReference()) {
                Map<String, Object> reference = new LinkedHashMap<>();
                reference.put("$ref", ref.getUri());
                return reference;
            }
            Object embedded = ref.getValue();
            if (embedded instanceof KanonakNode node
                && node.getId() != null && !node.getId().isEmpty()) {
                throw new IllegalArgumentException(
                    "An embedded value must not carry an id (derived identity, no $id) — "
                        + "to point at the named resource, use Ref.to(...).");
            }
            return toWireMap(embedded);
        }
        if (raw instanceof List<?> list) {
            List<Object> items = new ArrayList<>(list.size());
            for (Object item : list) {
                if (item == null) {
                    throw new IllegalArgumentException("A list-valued property cannot carry a null item.");
                }
                items.add(wireValue(item));
            }
            return items;
        }
        if (raw instanceof KanonakNode) {
            return toWireMap(raw);
        }
        if (raw instanceof Map<?, ?> map) {
            Map<String, Object> out = new LinkedHashMap<>();
            for (Map.Entry<?, ?> e : map.entrySet()) {
                if (e.getValue() == null) {
                    continue;
                }
                out.put(String.valueOf(e.getKey()), wireValue(e.getValue()));
            }
            return out;
        }
        throw new IllegalArgumentException(
            "Unsupported typed value " + raw.getClass().getName()
                + " — expected String/Boolean/Number/JsonNumber, Ref, List, KanonakNode, or Map.");
    }

    private static void putEnvelope(Map<String, Object> wire, String key, String value) {
        if (value != null && !value.isEmpty()) {
            wire.put(key, value);
        }
    }
}
