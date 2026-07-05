package org.kanonak.codec;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The metadata a generated SDK embeds for its codec — describes the SDK's OWN
 * classes (keyed by their durable URIs) plus the resolved foundation URIs. It
 * does NOT carry a package identity: the classes belong to the SDK's package,
 * but the instances a consumer builds live in the consumer's own (data) package,
 * whose identity is supplied at call time via {@link PackageContext}.
 *
 * <ul>
 *   <li>{@code typePredicate} — resolved {@code kanonak.org/core-rdf@<ver>/type} URI.</li>
 *   <li>{@code labelPredicate} — resolved {@code kanonak.org/core-rdf@<ver>/label} URI.</li>
 *   <li>{@code packageTypeUri} — resolved {@code kanonak.org/core-kanonak@<ver>/Package} URI.</li>
 *   <li>{@code classes} — classes keyed by durable type URI (the node's {@code $type}).</li>
 * </ul>
 */
public record CodecSchema(
    String typePredicate,
    String labelPredicate,
    String packageTypeUri,
    Map<String, CodecClass> classes
) {
    /** A class's canonicalization schema: its durable URI + its (flattened) props. */
    public record CodecClass(String typeUri, Map<String, CodecProp> props) {}

    /**
     * Parse the embedded schema JSON (the form the generators emit and the shared
     * vectors carry) into a {@link CodecSchema}. Generated SDKs embed their schema
     * as a JSON string and call this once at load. JDK-only: a minimal embedded
     * JSON parser reads the schema (only strings/objects appear — durable URIs and
     * the class/prop maps), so the generated codec needs no JSON library at runtime.
     * This is the single public deserializer the generated codec binds to.
     */
    @SuppressWarnings("unchecked")
    public static CodecSchema fromJson(String json) {
        Map<String, Object> s = (Map<String, Object>) Json.parse(json);
        Map<String, CodecClass> classes = new LinkedHashMap<>();
        Map<String, Object> rawClasses = (Map<String, Object>) s.get("classes");
        if (rawClasses != null) {
            for (Map.Entry<String, Object> e : rawClasses.entrySet()) {
                Map<String, Object> c = (Map<String, Object>) e.getValue();
                Map<String, CodecProp> props = new LinkedHashMap<>();
                Map<String, Object> rawProps = (Map<String, Object>) c.get("props");
                if (rawProps != null) {
                    for (Map.Entry<String, Object> pe : rawProps.entrySet()) {
                        Map<String, Object> p = (Map<String, Object>) pe.getValue();
                        props.put(pe.getKey(), new CodecProp(
                            (String) p.get("predicate"),
                            (String) p.get("kind"),
                            (String) p.get("datatype"),
                            (String) p.get("range")));
                    }
                }
                classes.put(e.getKey(), new CodecClass((String) c.get("typeUri"), props));
            }
        }
        return new CodecSchema(
            (String) s.get("typePredicate"),
            (String) s.get("labelPredicate"),
            (String) s.get("packageTypeUri"),
            classes);
    }

    /**
     * Minimal JDK-only JSON parser for the embedded schema. Strings, objects,
     * arrays, booleans, null, and numbers (as {@link String} tokens) — enough to
     * read the schema the generators emit without a runtime JSON dependency.
     */
    private static final class Json {
        private final String s;
        private int i;

        private Json(String s) {
            this.s = s;
        }

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
            Map<String, Object> m = new LinkedHashMap<>();
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

        private String number() {
            int start = i;
            while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
            return s.substring(start, i);
        }

        private void ws() {
            while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++;
        }
    }

    /**
     * One property's canonicalization metadata, as embedded by the generator.
     *
     * <ul>
     *   <li>{@code predicate} — the predicate's durable canonical URI (resolved version).</li>
     *   <li>{@code kind} — {@code "datatype"} (typed scalar) vs {@code "object"} (reference/embedded).</li>
     *   <li>{@code datatype} — the datatype's canonical URI (carrier source); present for datatype props.</li>
     *   <li>{@code range} — the range class's canonical URI (0.2.0); optionally present for object
     *       props. Maps an embedded value's fields when the embedded carries no explicit
     *       {@code $type} (range-derived typing: inference only, never materialized as a
     *       statement); may be {@code null}.</li>
     * </ul>
     */
    public record CodecProp(String predicate, String kind, String datatype, String range) {
        /** Convenience constructor for a prop without a range. */
        public CodecProp(String predicate, String kind, String datatype) {
            this(predicate, kind, datatype, null);
        }
    }
}
