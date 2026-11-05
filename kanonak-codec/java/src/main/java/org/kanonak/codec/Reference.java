package org.kanonak.codec;

/**
 * A reference to a named resource, held in a class of the caller's own (runtime#66). Generated code holds a member of
 * an enumeration this way: a member is always referenced and never embedded, and a field of such a class takes only
 * that enumeration's members, where a {@link Ref} takes any URI or an embedded value. The typed path writes it as
 * {@code {"$ref": uri}}, the wire form {@link Ref#to(String)} takes, so the two hash the same.
 */
public interface Reference {
    /** The referenced resource's canonical URI. */
    String uri();
}
