import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;

import org.kanonak.wire.WireError;
import org.kanonak.wire.WireReader;
import org.kanonak.wire.WireWriter;

/**
 * Drives the shared wire vectors through the Java kanonak-wire port. Read
 * vectors run an op-script against a hex buffer asserting values or required
 * errors {kind, offset}; write vectors run writer ops asserting exact output
 * bytes. Cases requiring a capability Java lacks are skipped WITH a reported
 * skip count.
 */
public final class Conformance {
    // Java has wide numeric params (int/long can exceed the wire range) and
    // UTF-16 strings (a String can hold an unpaired surrogate); it lacks
    // dynamic-numeric (2.5 is not representable in int).
    static final Set<String> CAPABILITIES = Set.of("wide-numeric-params", "utf16-strings");

    static int pass = 0;
    static int fail = 0;
    static int skipped = 0;

    public static void main(String[] args) throws Exception {
        String vdir = args.length > 0 ? args[0] : "../vectors";
        int fails = run(Paths.get(vdir, "wire-vectors.json"));
        System.exit(fails == 0 ? 0 : 1);
    }

    @SuppressWarnings("unchecked")
    static int run(Path path) throws Exception {
        Map<String, Object> doc = (Map<String, Object>) Json.parse(Files.readString(path, StandardCharsets.UTF_8));
        List<Object> readVectors = (List<Object>) doc.get("readVectors");
        List<Object> writeVectors = (List<Object>) doc.get("writeVectors");

        for (Object o : readVectors) {
            runReadVector((Map<String, Object>) o);
        }
        for (Object o : writeVectors) {
            runWriteVector((Map<String, Object>) o);
        }

        int total = readVectors.size() + writeVectors.size();
        System.out.println("wire-vectors: " + pass + "/" + total + " pass (" + skipped + " skipped)");
        if (fail > 0) {
            System.err.println(fail + " VECTOR(S) FAILED");
            return fail;
        }
        System.out.println("ALL VECTORS PASS");
        return 0;
    }

    static void failCase(String id, String msg) {
        fail++;
        System.err.println(id + ": " + msg);
    }

    @SuppressWarnings("unchecked")
    static boolean checkError(String id, String opName, RuntimeException e, Map<String, Object> want) {
        if (!(e instanceof WireError we)) {
            failCase(id, opName + ": threw a non-WireError: " + e.getMessage());
            return false;
        }
        String wantKind = (String) want.get("kind");
        if (!we.kind.equals(wantKind)) {
            failCase(id, opName + ": expected error kind " + wantKind + ", got " + we.kind + " (" + we.getMessage() + ")");
            return false;
        }
        Object wantOffset = want.get("offset");
        if (wantOffset != null) {
            Integer wanted = ((Number) wantOffset).intValue();
            if (!wanted.equals(we.offset)) {
                failCase(id, opName + ": expected error offset " + wanted + ", got " + we.offset);
                return false;
            }
        }
        return true;
    }

    // -- Read vectors -----------------------------------------------------

    /** Returns Long for numeric reads, String for byte/text reads, null for void ops. */
    static Object runReadOp(WireReader r, Map<String, Object> op) {
        String name = (String) op.get("op");
        switch (name) {
            case "u8": return (long) r.u8();
            case "u16be": return (long) r.u16be();
            case "u32be": return r.u32be();
            case "bytes": return hex(r.bytes(((Number) op.get("n")).intValue()));
            case "uuid": return r.uuid();
            case "utf8": return r.utf8(((Number) op.get("n")).intValue());
            case "lenPrefixedBytes16": return hex(r.lenPrefixedBytes16());
            case "rest": return hex(r.rest());
            case "remaining": return (long) r.remaining();
            case "expectEnd": r.expectEnd(); return null;
            default: throw new IllegalStateException("conformance: unknown read op '" + name + "'");
        }
    }

    @SuppressWarnings("unchecked")
    static void runReadVector(Map<String, Object> v) {
        String requires = (String) v.get("requires");
        if (requires != null && !CAPABILITIES.contains(requires)) {
            skipped++;
            return;
        }
        String id = (String) v.get("id");
        WireReader r = new WireReader(hexToBytes((String) v.get("bytes")));
        boolean ok = true;
        for (Object o : (List<Object>) v.get("ops")) {
            Map<String, Object> op = (Map<String, Object>) o;
            String opName = (String) op.get("op");
            Map<String, Object> expectError = (Map<String, Object>) op.get("expectError");
            if (expectError != null) {
                try {
                    runReadOp(r, op);
                    failCase(id, opName + ": expected " + expectError.get("kind") + ", got a value");
                    ok = false;
                } catch (RuntimeException e) {
                    if (!checkError(id, opName, e, expectError)) ok = false;
                }
                break; // an error op ends the script
            }
            Object got;
            try {
                got = runReadOp(r, op);
            } catch (RuntimeException e) {
                failCase(id, opName + ": threw " + e.getMessage());
                ok = false;
                break;
            }
            Object expected = op.get("expected");
            if (expected != null && !valueMatches(expected, got)) {
                failCase(id, opName + ": expected " + expected + ", got " + got);
                ok = false;
                break;
            }
        }
        if (ok) pass++;
    }

    static boolean valueMatches(Object expected, Object got) {
        if (expected instanceof Number n) {
            return got instanceof Long l && n.longValue() == l;
        }
        return expected.equals(got);
    }

    // -- Write vectors ----------------------------------------------------

    static void runWriteOp(WireWriter w, Map<String, Object> op) {
        String name = (String) op.get("op");
        switch (name) {
            case "u8": w.u8(((Number) op.get("value")).intValue()); return;
            case "u16be": w.u16be(((Number) op.get("value")).intValue()); return;
            case "u32be": w.u32be(((Number) op.get("value")).longValue()); return;
            case "bytes": w.bytes(hexToBytes((String) op.get("hex"))); return;
            case "uuid": w.uuid((String) op.get("value")); return;
            case "utf8": w.utf8(utf8OpString(op)); return;
            case "lenPrefixedBytes16": w.lenPrefixedBytes16(hexToBytes((String) op.get("hex"))); return;
            default: throw new IllegalStateException("conformance: unknown write op '" + name + "'");
        }
    }

    @SuppressWarnings("unchecked")
    static String utf8OpString(Map<String, Object> op) {
        List<Object> codeUnits = (List<Object>) op.get("utf16CodeUnits");
        if (codeUnits == null) {
            return (String) op.get("value");
        }
        char[] chars = new char[codeUnits.size()];
        for (int i = 0; i < chars.length; i++) {
            chars[i] = (char) ((Number) codeUnits.get(i)).intValue();
        }
        return new String(chars);
    }

    @SuppressWarnings("unchecked")
    static void runWriteVector(Map<String, Object> v) {
        String requires = (String) v.get("requires");
        if (requires != null && !CAPABILITIES.contains(requires)) {
            skipped++;
            return;
        }
        String id = (String) v.get("id");
        WireWriter w = new WireWriter();
        boolean ok = true;
        for (Object o : (List<Object>) v.get("ops")) {
            Map<String, Object> op = (Map<String, Object>) o;
            String opName = (String) op.get("op");
            Map<String, Object> expectError = (Map<String, Object>) op.get("expectError");
            if (expectError != null) {
                try {
                    runWriteOp(w, op);
                    failCase(id, opName + ": expected " + expectError.get("kind") + ", got success");
                    ok = false;
                } catch (RuntimeException e) {
                    if (!checkError(id, opName, e, expectError)) ok = false;
                }
                break;
            }
            try {
                runWriteOp(w, op);
            } catch (RuntimeException e) {
                failCase(id, opName + ": threw " + e.getMessage());
                ok = false;
                break;
            }
        }
        String expectedBytes = (String) v.get("expectedBytes");
        if (ok && expectedBytes != null) {
            String got = hex(w.toBytes());
            if (!got.equals(expectedBytes)) {
                failCase(id, "expected bytes " + expectedBytes + ", got " + got);
                ok = false;
            }
        }
        if (ok) pass++;
    }

    // -- Hex helpers ------------------------------------------------------

    static byte[] hexToBytes(String hex) {
        byte[] out = new byte[hex.length() / 2];
        for (int i = 0; i < out.length; i++) {
            out[i] = (byte) Integer.parseInt(hex.substring(i * 2, i * 2 + 2), 16);
        }
        return out;
    }

    static String hex(byte[] b) {
        StringBuilder sb = new StringBuilder(b.length * 2);
        for (byte x : b) {
            sb.append(String.format("%02x", x & 0xFF));
        }
        return sb.toString();
    }

    static String hex(ByteBuffer b) {
        StringBuilder sb = new StringBuilder(b.remaining() * 2);
        for (int i = b.position(); i < b.limit(); i++) {
            sb.append(String.format("%02x", b.get(i) & 0xFF));
        }
        return sb.toString();
    }

    // -- Minimal JSON parser (objects/arrays/strings/numbers/booleans/null) ----

    static final class Json {
        private final String s;
        private int i;

        private Json(String s) { this.s = s; }

        static Object parse(String s) {
            Json j = new Json(s);
            j.ws();
            Object v = j.value();
            j.ws();
            return v;
        }

        private Object value() {
            char c = s.charAt(i);
            switch (c) {
                case '{': return object();
                case '[': return array();
                case '"': return string();
                case 't': i += 4; return Boolean.TRUE;
                case 'f': i += 5; return Boolean.FALSE;
                case 'n': i += 4; return null;
                default: return number();
            }
        }

        private Map<String, Object> object() {
            Map<String, Object> m = new java.util.LinkedHashMap<>();
            i++; ws();
            if (s.charAt(i) == '}') { i++; return m; }
            while (true) {
                ws();
                String key = string();
                ws(); i++; // ':'
                ws();
                m.put(key, value());
                ws();
                char c = s.charAt(i++);
                if (c == '}') return m;
            }
        }

        private List<Object> array() {
            List<Object> a = new ArrayList<>();
            i++; ws();
            if (s.charAt(i) == ']') { i++; return a; }
            while (true) {
                ws();
                a.add(value());
                ws();
                char c = s.charAt(i++);
                if (c == ']') return a;
            }
        }

        private String string() {
            i++; // opening quote
            StringBuilder sb = new StringBuilder();
            while (true) {
                char c = s.charAt(i++);
                if (c == '"') return sb.toString();
                if (c == '\\') {
                    char e = s.charAt(i++);
                    switch (e) {
                        case '"': sb.append('"'); break;
                        case '\\': sb.append('\\'); break;
                        case '/': sb.append('/'); break;
                        case 'b': sb.append('\b'); break;
                        case 'f': sb.append('\f'); break;
                        case 'n': sb.append('\n'); break;
                        case 'r': sb.append('\r'); break;
                        case 't': sb.append('\t'); break;
                        case 'u':
                            sb.append((char) Integer.parseInt(s.substring(i, i + 4), 16));
                            i += 4;
                            break;
                        default: sb.append(e);
                    }
                } else {
                    sb.append(c);
                }
            }
        }

        private Double number() {
            int start = i;
            while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
            return Double.parseDouble(s.substring(start, i));
        }

        private void ws() {
            while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++;
        }
    }
}
