package org.kanonak.canonical;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/**
 * The frozen canonical structural form + content hash (canonicalFormVersion "1").
 * Subjects are ordered by the UTF-8 byte sequence of their URI, statements by the
 * predicate URI; lists keep source order. The wire form is compact JSON with RFC
 * 8785 escaping and a fixed per-blob field order; the content address is the
 * SHA-256 of those bytes, prefixed {@code sha256:}.
 */
public final class CanonicalForm {
    private CanonicalForm() {}

    public static final String CANONICAL_FORM_VERSION = "1";

    // -- Value model (the typed-value representation the canonical form consumes) --

    public sealed interface Value permits Value.Typed, Value.Raw, Value.Ref, Value.Embed, Value.KList {
        /** A datatype-typed scalar: carrier tag + raw lexical token (canonicalized on serialize). */
        record Typed(Carrier carrier, String lexical) implements Value {}
        /** An untyped / open-world scalar: the raw token preserved verbatim. */
        record Raw(String token) implements Value {}
        /** A reference to an entity by its full canonical URI. */
        record Ref(String uri) implements Value {}
        /** An embedded node (optional dict-key name + its statements). */
        record Embed(String name, List<Statement> statements) implements Value {}
        /** An ordered list (source order preserved). */
        record KList(List<Value> items) implements Value {}
    }

    public record Statement(String predicate, Value value) {}
    public record Subject(String uri, List<Statement> statements) {}
    public record Package(List<Subject> subjects) {}

    private static final Comparator<byte[]> UTF8 = (a, b) -> {
        int n = Math.min(a.length, b.length);
        for (int i = 0; i < n; i++) {
            int d = (a[i] & 0xff) - (b[i] & 0xff);
            if (d != 0) return d;
        }
        return a.length - b.length;
    };

    public static String serialize(Package pkg) {
        StringBuilder sb = new StringBuilder();
        sb.append("{\"subjects\":[");
        List<Subject> subjects = new ArrayList<>(pkg.subjects());
        subjects.sort(Comparator.comparing(s -> s.uri().getBytes(StandardCharsets.UTF_8), UTF8));
        for (int i = 0; i < subjects.size(); i++) {
            if (i > 0) sb.append(',');
            Subject s = subjects.get(i);
            sb.append("{\"uri\":");
            emitString(sb, s.uri());
            sb.append(",\"statements\":[");
            emitStatements(sb, s.statements());
            sb.append("]}");
        }
        sb.append("]}");
        return sb.toString();
    }

    public static String hash(Package pkg) {
        try {
            byte[] digest = MessageDigest.getInstance("SHA-256")
                .digest(serialize(pkg).getBytes(StandardCharsets.UTF_8));
            StringBuilder hex = new StringBuilder(digest.length * 2);
            for (byte b : digest) hex.append(String.format("%02x", b));
            return "sha256:" + hex;
        } catch (java.security.NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void emitStatements(StringBuilder sb, List<Statement> stmts) {
        List<Statement> ordered = new ArrayList<>(stmts);
        ordered.sort(Comparator.comparing(s -> s.predicate().getBytes(StandardCharsets.UTF_8), UTF8));
        for (int i = 0; i < ordered.size(); i++) {
            if (i > 0) sb.append(',');
            Statement st = ordered.get(i);
            sb.append("{\"predicate\":");
            emitString(sb, st.predicate());
            sb.append(',');
            emitValueTail(sb, st.value());
            sb.append('}');
        }
    }

    private static void emitValueTail(StringBuilder sb, Value v) {
        if (v instanceof Value.Typed t) {
            sb.append("\"type\":\"typed\",\"carrier\":");
            emitString(sb, t.carrier().tag());
            sb.append(",\"value\":");
            emitString(sb, Datatypes.canonicalScalarLexical(t.carrier(), t.lexical()));
        } else if (v instanceof Value.Raw r) {
            sb.append("\"type\":\"string\",\"value\":");
            emitString(sb, r.token());
        } else if (v instanceof Value.Ref rf) {
            sb.append("\"type\":\"ref\",\"value\":");
            emitString(sb, rf.uri());
        } else if (v instanceof Value.Embed e) {
            sb.append("\"type\":\"embedded\"");
            if (e.name() != null) {
                sb.append(",\"name\":");
                emitString(sb, e.name());
            }
            sb.append(",\"statements\":[");
            emitStatements(sb, e.statements());
            sb.append(']');
        } else if (v instanceof Value.KList l) {
            sb.append("\"type\":\"list\",\"items\":[");
            for (int i = 0; i < l.items().size(); i++) {
                if (i > 0) sb.append(',');
                sb.append('{');
                emitValueTail(sb, l.items().get(i));
                sb.append('}');
            }
            sb.append(']');
        } else {
            throw new IllegalArgumentException("canonicalForm: unrecognized value kind");
        }
    }

    /** RFC 8785 / JSON.stringify string escaping. */
    private static void emitString(StringBuilder sb, String s) {
        sb.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"': sb.append("\\\""); break;
                case '\\': sb.append("\\\\"); break;
                case '\b': sb.append("\\b"); break;
                case '\f': sb.append("\\f"); break;
                case '\n': sb.append("\\n"); break;
                case '\r': sb.append("\\r"); break;
                case '\t': sb.append("\\t"); break;
                default:
                    if (c < 0x20) sb.append(String.format("\\u%04x", (int) c));
                    else sb.append(c);
            }
        }
        sb.append('"');
    }
}
