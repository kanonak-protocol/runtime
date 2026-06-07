package org.kanonak.codec;

/**
 * A JSON numeric literal that retains its exact source token, so the codec's
 * {@code lexical()} of a number is byte-exact (e.g. {@code 5} → {@code "5"},
 * {@code 1.5} → {@code "1.5"}) with no locale or trailing-zero/scientific
 * artifacts introduced by parsing through a binary float. A node may carry a
 * number as a {@code JsonNumber} (from a JSON parser that retains tokens) or as a
 * plain {@link Number}; the codec handles both.
 */
public record JsonNumber(String token) implements java.io.Serializable {
    @Override
    public String toString() {
        return token;
    }
}
