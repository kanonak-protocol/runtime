package org.kanonak.canonical;

import java.util.HashMap;
import java.util.Map;

/**
 * The closed set of canonical-form carriers (canonicalFormVersion "1") and the
 * normative datatype-URI → carrier routing. A datatype maps to exactly one
 * carrier; the carrier determines both the canonical lexical rule and the tag
 * that participates in identity.
 */
public enum Carrier {
    INTEGER("integer"),
    DECIMAL("decimal"),
    DOUBLE("double"),
    FLOAT("float"),
    BOOLEAN("boolean"),
    STRING("string"),
    ANY_URI("anyURI"),
    LANG_STRING("langString"),
    DATE_TIME("dateTime"),
    DATE("date"),
    TIME("time"),
    HEX_BINARY("hexBinary"),
    BASE64_BINARY("base64Binary");

    private final String tag;

    Carrier(String tag) {
        this.tag = tag;
    }

    /** The wire tag (the exact string that appears in the canonical form). */
    public String tag() {
        return tag;
    }

    private static final Map<String, Carrier> BY_TAG = new HashMap<>();
    private static final Map<String, Carrier> BY_XSD_NAME = new HashMap<>();

    static {
        for (Carrier c : values()) BY_TAG.put(c.tag, c);
        String[][] xsd = {
            {"integer", "INTEGER"}, {"long", "INTEGER"}, {"int", "INTEGER"}, {"short", "INTEGER"},
            {"byte", "INTEGER"}, {"unsignedLong", "INTEGER"}, {"unsignedInt", "INTEGER"},
            {"unsignedShort", "INTEGER"}, {"unsignedByte", "INTEGER"}, {"nonNegativeInteger", "INTEGER"},
            {"positiveInteger", "INTEGER"}, {"nonPositiveInteger", "INTEGER"}, {"negativeInteger", "INTEGER"},
            {"decimal", "DECIMAL"}, {"double", "DOUBLE"}, {"float", "FLOAT"}, {"boolean", "BOOLEAN"},
            {"string", "STRING"}, {"normalizedString", "STRING"}, {"token", "STRING"}, {"anyURI", "ANY_URI"},
            {"dateTime", "DATE_TIME"}, {"date", "DATE"}, {"time", "TIME"},
            {"hexBinary", "HEX_BINARY"}, {"base64Binary", "BASE64_BINARY"},
        };
        for (String[] e : xsd) BY_XSD_NAME.put(e[0], Carrier.valueOf(e[1]));
    }

    /** Carrier for a tag string, or null. */
    public static Carrier fromTag(String tag) {
        return BY_TAG.get(tag);
    }

    /**
     * Carrier for a datatype URI (publisher/package@ver/name or publisher/package/name),
     * or null if outside the v1 canonicalized set (the untyped/raw-token tier).
     */
    public static Carrier of(String datatypeUri) {
        int idx = datatypeUri.lastIndexOf('/');
        String name = datatypeUri.substring(idx + 1);
        String head = datatypeUri.substring(0, idx);
        int slash = head.indexOf('/');
        String publisher = head.substring(0, slash);
        String pkg = head.substring(slash + 1);
        int at = pkg.indexOf('@');
        if (at >= 0) pkg = pkg.substring(0, at);
        String key = publisher + "/" + pkg + "/" + name;
        if (key.equals("kanonak.org/core-rdf/langString")) return LANG_STRING;
        if (!key.startsWith("kanonak.org/core-xsd/")) return null;
        return BY_XSD_NAME.get(name);
    }
}
