package org.kanonak.wire;

/**
 * A wire kernel error: what was expected, what was found, and where.
 *
 * <p>The error taxonomy of {@code wireFormatVersion "1"} — {@code Truncated},
 * {@code LengthOverrun}, {@code TrailingBytes}, {@code InvalidUtf8},
 * {@code InvalidUuid}, {@code ValueOutOfRange}, {@code UnknownTag}. Every
 * message states what was expected, what was found, and where. There are no
 * silent fallbacks: no null returns, no partial values, no lossy decodes.
 *
 * <p>{@link #WIRE_FORMAT_VERSION} freezes the wire contract; a change to any
 * rule requires a NEW version, never an edit in place.
 */
public final class WireError extends RuntimeException {
    /** The frozen wire-format version (the determinism contract). */
    public static final String WIRE_FORMAT_VERSION = "1";

    /** The error kind, one of the seven taxonomy names (e.g. {@code "Truncated"}). */
    public final String kind;

    /**
     * Absolute byte offset where the failing read started (read-side errors);
     * {@code null} when not applicable (writer-side errors).
     */
    public final Integer offset;

    private WireError(String kind, String message, Integer offset) {
        super(message);
        this.kind = kind;
        this.offset = offset;
    }

    public String getKind() {
        return kind;
    }

    public Integer getOffset() {
        return offset;
    }

    public static WireError truncated(int needed, int remaining, int offset, String context) {
        return new WireError(
            "Truncated",
            "Truncated: " + context + " needs " + needed + " byte(s) at offset " + offset
                + ", " + remaining + " remain",
            offset);
    }

    public static WireError lengthOverrun(int declared, int remaining, int offset, String context) {
        return new WireError(
            "LengthOverrun",
            "LengthOverrun: " + context + " at offset " + offset + " declares " + declared
                + " byte(s), " + remaining + " remain after the length field",
            offset);
    }

    public static WireError trailingBytes(int count, int offset) {
        return new WireError(
            "TrailingBytes",
            "TrailingBytes: expected end of buffer at offset " + offset + ", " + count
                + " byte(s) remain",
            offset);
    }

    public static WireError invalidUtf8(Integer offset, String context) {
        return new WireError(
            "InvalidUtf8",
            offset == null
                ? "InvalidUtf8: " + context
                : "InvalidUtf8: " + context + " at offset " + offset,
            offset);
    }

    public static WireError invalidUuid(String context) {
        return new WireError("InvalidUuid", "InvalidUuid: " + context, null);
    }

    public static WireError valueOutOfRange(long value, String type) {
        return new WireError("ValueOutOfRange", "ValueOutOfRange: " + value + " is not a valid " + type, null);
    }

    /** Constructor for generated union dispatch on an unrecognized tag byte. */
    public static WireError unknownTag(int tag, String context) {
        return new WireError(
            "UnknownTag",
            String.format("UnknownTag: 0x%02x is not a known %s", tag & 0xFF, context),
            null);
    }
}
