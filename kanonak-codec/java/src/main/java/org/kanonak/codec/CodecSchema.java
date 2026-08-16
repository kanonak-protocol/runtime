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
    Map<String, CodecClass> classes,
    Map<String, CodecEnum> enums
) {
    /**
     * Source-compatible 4-arg form for callers written before 0.5.0 added
     * {@code enums}. Yields a schema declaring no enumerations.
     */
    public CodecSchema(String typePredicate, String labelPredicate,
                       String packageTypeUri, Map<String, CodecClass> classes) {
        this(typePredicate, labelPredicate, packageTypeUri, classes, Map.of());
    }

    /** A class's canonicalization schema: its durable URI + its (flattened) props. */
    public record CodecClass(String typeUri, Map<String, CodecProp> props) {}

    /**
     * One member of a closed set of named individuals. A record even when
     * {@code label} is its only component, so a later addition (an ordered
     * scale's dominates closure, runtime#16) stays additive rather than a
     * breaking shape change.
     *
     * <p>{@code label} is the ontology's own, read through the same predicate
     * {@link #labelPredicate()} names. Display-lens resolution is a RENDERING
     * concern and deliberately not here.
     */
    public record CodecEnumMember(String label) {}

    /**
     * A closed set of named individuals — the metadata that lets a consumer
     * holding only the schema resolve a member URI it received on the wire,
     * with no generated SDK for the declaring package.
     *
     * <p>Carried, never enforced. A prop ranged at an enumeration in ANOTHER
     * package has no entry here ({@code classes} and {@code enums} both
     * describe the SDK's OWN package), so a codec that validated a {@code $ref}
     * against these members would false-reject valid cross-package data.
     * Absence means "not mine", never "invalid".
     *
     * <p>{@code members} is keyed by durable VERSIONED URI
     * ({@code publisher/package@version/name}), byte-identical to what the wire
     * carries as {@code {"$ref": ...}}. A versionless key would look right and
     * miss every lookup, so the formation is an invariant, not a convention.
     * The local name is NOT duplicated as a key: derive it from the URI with
     * canonical's local-name parser (runtime#17).
     *
     * <p>What makes a set closed is the PRODUCER's decision, taken in the
     * ontology and resolved by the schema layer above; this runtime carries the
     * outcome and names no vocabulary term.
     */
    public record CodecEnum(String typeUri, Map<String, CodecEnumMember> members) {}

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
        // Closed sets (0.5.0, runtime#21). Optional: a schema written before
        // this field remains valid and yields an empty map.
        Map<String, CodecEnum> enums = new LinkedHashMap<>();
        Map<String, Object> rawEnums = (Map<String, Object>) s.get("enums");
        if (rawEnums != null) {
            for (Map.Entry<String, Object> e : rawEnums.entrySet()) {
                Map<String, Object> en = (Map<String, Object>) e.getValue();
                Map<String, CodecEnumMember> members = new LinkedHashMap<>();
                Map<String, Object> rawMembers = (Map<String, Object>) en.get("members");
                if (rawMembers != null) {
                    for (Map.Entry<String, Object> me : rawMembers.entrySet()) {
                        Map<String, Object> m = (Map<String, Object>) me.getValue();
                        members.put(me.getKey(), new CodecEnumMember((String) m.get("label")));
                    }
                }
                enums.put(e.getKey(), new CodecEnum((String) en.get("typeUri"), members));
            }
        }
        return new CodecSchema(
            (String) s.get("typePredicate"),
            (String) s.get("labelPredicate"),
            (String) s.get("packageTypeUri"),
            classes,
            enums);
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
