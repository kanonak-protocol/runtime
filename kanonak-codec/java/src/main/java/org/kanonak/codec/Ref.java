package org.kanonak.codec;

import java.util.Objects;

/**
 * An object property's value: EXACTLY ONE of a reference to a named resource
 * (its canonical URI) or an embedded node (the value itself, carried inline —
 * derived identity, no {@code $id}). This is the typed twin of the wire form's
 * {@code {"$ref": uri}} vs embedded-node distinction; the choice between the
 * arms is authorial and hash-relevant, so it is explicit here, never inferred.
 */
public final class Ref<T> {
    private final String uri;
    private final T value;

    private Ref(String uri, T value) {
        this.uri = uri;
        this.value = value;
    }

    /** The referenced resource's canonical URI — the reference arm (else {@code null}). */
    public String getUri() {
        return uri;
    }

    /** The embedded value — the embedded arm (else {@code null}). */
    public T getValue() {
        return value;
    }

    /** True when this is the reference arm. */
    public boolean isReference() {
        return uri != null;
    }

    /** A reference to a named resource by its canonical URI. */
    public static <T> Ref<T> to(String uri) {
        if (uri == null || uri.isEmpty()) {
            throw new IllegalArgumentException("A reference needs a canonical URI.");
        }
        return new Ref<>(uri, null);
    }

    /**
     * A reference to a named resource by the instance itself — resolved
     * through the target's {@link KanonakNode#getId() id}. The target must
     * already carry its identity; an embedded (id-less) value cannot be
     * referenced (embed it, or give it a name at the package level).
     */
    public static <T> Ref<T> to(KanonakNode target) {
        Objects.requireNonNull(target, "target");
        if (target.getId() == null || target.getId().isEmpty()) {
            throw new IllegalArgumentException(
                "Ref.to(target) requires a KanonakNode with a non-empty id — "
                    + "to carry the value inline instead, use Ref.embed(value).");
        }
        return new Ref<>(target.getId(), null);
    }

    /** An embedded value, carried inline (derived identity, no {@code $id}). */
    public static <T> Ref<T> embed(T value) {
        Objects.requireNonNull(value, "value");
        return new Ref<>(null, value);
    }

    /** An embedded value with its authored dict-key name (hash-relevant — rides {@code $name}). */
    public static <T> Ref<T> embed(T value, String name) {
        Objects.requireNonNull(value, "value");
        if (!(value instanceof KanonakNode node)) {
            throw new IllegalArgumentException(
                "Naming an embedded value requires a KanonakNode (the name rides $name).");
        }
        node.setName(name);
        return new Ref<>(null, value);
    }
}
