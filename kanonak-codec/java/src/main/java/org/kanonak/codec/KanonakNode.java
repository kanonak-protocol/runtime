package org.kanonak.codec;

import java.util.Map;

/**
 * The {@code $}-envelope as data — the base class a generated typed model
 * extends so an instance carries its own identity and binds straight to the
 * normalized-JSON wire form via {@link TypedNodes}. Envelope keys are reserved
 * (never ontology statements); {@link #getExtra() extra} holds the open-world
 * assertions outside the type-model, keyed by predicate URI, and rides the
 * wire as sibling fields (matching {@link Codec#serialize} semantics).
 */
public abstract class KanonakNode {
    private String id;
    private String typeUri;
    private String name;
    private String packageContentHash;
    private String packageVersion;
    private Map<String, Object> extra;

    /** The resource's canonical URI ({@code $id}). Required to form a subject. */
    public String getId() {
        return id;
    }

    public void setId(String id) {
        this.id = id;
    }

    /** The durable class URI ({@code $type}) — the value of the synthesized type triple. */
    public String getTypeUri() {
        return typeUri;
    }

    public void setTypeUri(String typeUri) {
        this.typeUri = typeUri;
    }

    /**
     * An embedded value's authored dict-key ({@code $name}) — HASH-RELEVANT
     * (serialized into the canonical form). Only meaningful when this instance
     * is used as an embedded value (via {@link Ref#embed(Object, String)});
     * {@code null} for subjects.
     */
    public String getName() {
        return name;
    }

    public void setName(String name) {
        this.name = name;
    }

    /** Package provenance on read ({@code $contentHash}); ignored if echoed back on write. */
    public String getPackageContentHash() {
        return packageContentHash;
    }

    public void setPackageContentHash(String packageContentHash) {
        this.packageContentHash = packageContentHash;
    }

    /** Package provenance on read ({@code $version}); ignored if echoed back on write. */
    public String getPackageVersion() {
        return packageVersion;
    }

    public void setPackageVersion(String packageVersion) {
        this.packageVersion = packageVersion;
    }

    /**
     * Open-world assertions outside the type-model, keyed by predicate URI.
     * Lossless round-trip: a consumer that ignores this still carries it.
     * May be {@code null} when the instance carries none.
     */
    public Map<String, Object> getExtra() {
        return extra;
    }

    public void setExtra(Map<String, Object> extra) {
        this.extra = extra;
    }
}
