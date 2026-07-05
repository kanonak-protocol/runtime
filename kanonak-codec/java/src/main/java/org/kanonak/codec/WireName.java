package org.kanonak.codec;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * A generated property field's wire name — the alias-collapsed local name the
 * field rides under in the normalized-JSON wire form. Generated SDK classes
 * annotate every property field; {@link TypedNodes} reads it reflectively at
 * bind time. The runtime is JDK-only, so it defines its own wire-name
 * annotation rather than depending on a JSON library's.
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.FIELD)
public @interface WireName {
    /** The wire-form local name. */
    String value();
}
