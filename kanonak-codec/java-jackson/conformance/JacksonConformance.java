import java.io.File;
import java.math.BigDecimal;
import java.math.BigInteger;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.TreeSet;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.json.JsonMapper;
import com.fasterxml.jackson.databind.annotation.JsonDeserialize;
import com.fasterxml.jackson.databind.annotation.JsonSerialize;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.kanonak.codec.KanonakNode;
import org.kanonak.codec.jackson.JsonForms;
import org.kanonak.codec.jackson.KanonakModule;

/**
 * The envelope binding, both ways, over every node the shared codec vectors
 * hold, as given and as serialized: each node is read by Jackson into a class
 * that binds no field of its own, and must give back each {@code $} key it
 * carries through KanonakNode's accessors, then be written with exactly those
 * keys and nothing of {@code extra}. The mapper finds the module as a consumer's
 * would, by {@code findAndAddModules}. A field's binding is the generated
 * class's, so the hash of a whole node is a generated package's round trip to
 * check, not this module's.
 */
public final class JacksonConformance {
    static final String[] ENVELOPE = {"$id", "$type", "$types", "$name", "$contentHash", "$version"};

    /** A generated class that binds no field: everything but the envelope is an unknown key. */
    static final class Bare extends KanonakNode {}

    public static void main(String[] args) throws Exception {
        File vectors = new File(args.length > 0 ? args[0] : "../vectors");
        ObjectMapper plain = new ObjectMapper();
        ObjectMapper mapper = JsonMapper.builder()
            .findAndAddModules()
            .disable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES)
            .build();
        if (!mapper.getRegisteredModuleIds().contains(KanonakModule.class.getName())) {
            throw new IllegalStateException("findAndAddModules did not find " + KanonakModule.class.getName()
                + ": its META-INF/services entry is missing from the classpath");
        }

        List<String> failures = new ArrayList<>();
        int checked = 0;
        int failed = 0;
        File[] files = vectors.listFiles((dir, name) -> name.startsWith("codec-vectors") && name.endsWith(".json"));
        if (files == null || files.length == 0) throw new IllegalStateException("no codec vectors in " + vectors.getAbsolutePath());
        for (File file : files) {
            JsonNode cases = plain.readTree(file).path("cases");
            for (JsonNode c : cases) {
                List<ObjectNode> nodes = new ArrayList<>();
                collect(c.path("nodes"), nodes);
                collect(c.path("expectedSerialize"), nodes);
                for (ObjectNode node : nodes) {
                    checked++;
                    String where = file.getName() + " " + c.path("id").asText() + " " + node.path("$id").asText(node.path("$name").asText("?"));
                    int before = failures.size();
                    check(mapper, node, where, failures);
                    if (failures.size() > before) failed++;
                }
            }
        }
        failures.forEach(f -> System.out.println("  FAIL " + f));
        System.out.println("jackson-envelope: " + (checked - failed) + "/" + checked + " nodes pass");
        if (checked == 0) throw new IllegalStateException("the codec vectors gave no node to check");
        failures.addAll(bindingFailures(vectors));
        if (!failures.isEmpty()) System.exit(1);
        System.out.println("\nALL VECTORS PASS");
    }

    /** Every JSON object below {@code value} that carries an envelope key, a reference ({@code $ref}) aside. */
    static void collect(JsonNode value, List<ObjectNode> out) {
        if (value.isArray()) {
            for (JsonNode item : value) collect(item, out);
        } else if (value.isObject()) {
            ObjectNode object = (ObjectNode) value;
            for (String key : ENVELOPE) {
                if (object.has(key)) { out.add(object); break; }
            }
            for (Map.Entry<String, JsonNode> field : object.properties()) collect(field.getValue(), out);
        }
    }

    static void check(ObjectMapper mapper, ObjectNode node, String where, List<String> failures) throws Exception {
        Bare read = mapper.treeToValue(node, Bare.class);
        Object[] got = {read.getId(), read.getTypeUri(), read.getTypes(), read.getName(), read.getPackageContentHash(), read.getPackageVersion()};
        Set<String> carried = new TreeSet<>();
        for (int i = 0; i < ENVELOPE.length; i++) {
            JsonNode expected = node.get(ENVELOPE[i]);
            Object want = expected == null || expected.isNull() ? null
                : expected.isArray() ? mapper.convertValue(expected, List.class) : expected.asText();
            if (want != null) carried.add(ENVELOPE[i]);
            if (!Objects.equals(want, got[i])) failures.add(where + ": " + ENVELOPE[i] + " read as " + got[i] + ", expected " + want);
        }

        read.setExtra(Map.of("example.org/open/world", "kept out"));
        JsonNode written = mapper.valueToTree(read);
        Set<String> keys = new TreeSet<>();
        written.fieldNames().forEachRemaining(keys::add);
        if (!keys.equals(carried)) failures.add(where + ": written with " + keys + ", expected exactly " + carried);
        for (String key : carried) {
            if (!Objects.equals(written.get(key), node.get(key))) {
                failures.add(where + ": " + key + " written as " + written.get(key) + ", expected " + node.get(key));
            }
        }
    }

    // -- The Jackson binding of the JSON forms (runtime#54) -----------------------
    // Each read and write case of codec-vectors-json-numbers.json through Jackson, each value on a field named
    // with its datatype's serializer and deserializer, as a generated class names them. Every refusal names
    // [json-number-form]; a write case whose value its Java type can't hold, such as a long past 64 bits, is
    // refused by the type itself.

    static final ObjectMapper PLAIN = new ObjectMapper();

    static final class FInteger {
        @JsonSerialize(using = JsonForms.XsdIntegerSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdIntegerDeserializer.class)
        public BigInteger v;
    }

    static final class FNonNegativeInteger {
        @JsonSerialize(using = JsonForms.XsdNonNegativeIntegerSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdNonNegativeIntegerDeserializer.class)
        public BigInteger v;
    }

    static final class FPositiveInteger {
        @JsonSerialize(using = JsonForms.XsdPositiveIntegerSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdPositiveIntegerDeserializer.class)
        public BigInteger v;
    }

    static final class FNonPositiveInteger {
        @JsonSerialize(using = JsonForms.XsdNonPositiveIntegerSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdNonPositiveIntegerDeserializer.class)
        public BigInteger v;
    }

    static final class FNegativeInteger {
        @JsonSerialize(using = JsonForms.XsdNegativeIntegerSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdNegativeIntegerDeserializer.class)
        public BigInteger v;
    }

    static final class FLong {
        @JsonSerialize(using = JsonForms.XsdLongSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdLongDeserializer.class)
        public Long v;
    }

    static final class FUnsignedLong {
        @JsonSerialize(using = JsonForms.XsdUnsignedLongSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdUnsignedLongDeserializer.class)
        public BigInteger v;
    }

    static final class FDecimal {
        @JsonSerialize(using = JsonForms.XsdDecimalSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdDecimalDeserializer.class)
        public BigDecimal v;
    }

    static final class FInt {
        @JsonSerialize(using = JsonForms.XsdIntSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdIntDeserializer.class)
        public Integer v;
    }

    static final class FShort {
        @JsonSerialize(using = JsonForms.XsdShortSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdShortDeserializer.class)
        public Short v;
    }

    static final class FByte {
        @JsonSerialize(using = JsonForms.XsdByteSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdByteDeserializer.class)
        public Byte v;
    }

    static final class FUnsignedInt {
        @JsonSerialize(using = JsonForms.XsdUnsignedIntSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdUnsignedIntDeserializer.class)
        public Long v;
    }

    static final class FUnsignedShort {
        @JsonSerialize(using = JsonForms.XsdUnsignedShortSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdUnsignedShortDeserializer.class)
        public Integer v;
    }

    static final class FUnsignedByte {
        @JsonSerialize(using = JsonForms.XsdUnsignedByteSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdUnsignedByteDeserializer.class)
        public Short v;
    }

    static final class FCounts {
        @JsonSerialize(contentUsing = JsonForms.XsdIntegerSerializer.class)
        @JsonDeserialize(contentUsing = JsonForms.XsdIntegerDeserializer.class)
        public List<BigInteger> v;
    }

    /** The embedded Amount, as a generated class holds it. */
    static final class Amount {
        @JsonSerialize(using = JsonForms.XsdDecimalSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdDecimalDeserializer.class)
        public BigDecimal value;
    }

    static final class FAmount {
        public Amount v;
    }

    /** core-owl's cardinality, an xsd:nonNegativeInteger, the field that asked for the binding. */
    static final class Restriction {
        @JsonSerialize(using = JsonForms.XsdNonNegativeIntegerSerializer.class)
        @JsonDeserialize(using = JsonForms.XsdNonNegativeIntegerDeserializer.class)
        public BigInteger cardinality;
    }

    /** What a read value reads as, compared with a case's expected: each value's canonical lexical form. */
    static Object lexical(Object v) {
        if (v instanceof BigDecimal d) return d.toPlainString();
        if (v instanceof List<?> list) return list.stream().map(JacksonConformance::lexical).toList();
        if (v instanceof Amount a) return Map.of("value", a.value.toPlainString());
        return String.valueOf(v);
    }

    static Object readAs(String field, String value) throws Exception {
        String json = "{\"v\":" + value + "}";
        return switch (field) {
            case "integer" -> PLAIN.readValue(json, FInteger.class).v;
            case "nonNegativeInteger" -> PLAIN.readValue(json, FNonNegativeInteger.class).v;
            case "positiveInteger" -> PLAIN.readValue(json, FPositiveInteger.class).v;
            case "nonPositiveInteger" -> PLAIN.readValue(json, FNonPositiveInteger.class).v;
            case "negativeInteger" -> PLAIN.readValue(json, FNegativeInteger.class).v;
            case "long" -> PLAIN.readValue(json, FLong.class).v;
            case "unsignedLong" -> PLAIN.readValue(json, FUnsignedLong.class).v;
            case "decimal" -> PLAIN.readValue(json, FDecimal.class).v;
            case "int" -> PLAIN.readValue(json, FInt.class).v;
            case "short" -> PLAIN.readValue(json, FShort.class).v;
            case "byte" -> PLAIN.readValue(json, FByte.class).v;
            case "unsignedInt" -> PLAIN.readValue(json, FUnsignedInt.class).v;
            case "unsignedShort" -> PLAIN.readValue(json, FUnsignedShort.class).v;
            case "unsignedByte" -> PLAIN.readValue(json, FUnsignedByte.class).v;
            case "counts" -> PLAIN.readValue(json, FCounts.class).v;
            case "amount" -> PLAIN.readValue(json, FAmount.class).v;
            default -> throw new IllegalStateException("no binding type for field " + field);
        };
    }

    static Object holderOf(String field, JsonNode value) {
        String t = value.asText();
        return switch (field) {
            case "integer" -> { FInteger f = new FInteger(); f.v = new BigInteger(t); yield f; }
            case "nonNegativeInteger" -> { FNonNegativeInteger f = new FNonNegativeInteger(); f.v = new BigInteger(t); yield f; }
            case "positiveInteger" -> { FPositiveInteger f = new FPositiveInteger(); f.v = new BigInteger(t); yield f; }
            case "nonPositiveInteger" -> { FNonPositiveInteger f = new FNonPositiveInteger(); f.v = new BigInteger(t); yield f; }
            case "negativeInteger" -> { FNegativeInteger f = new FNegativeInteger(); f.v = new BigInteger(t); yield f; }
            case "long" -> { FLong f = new FLong(); f.v = Long.parseLong(t); yield f; }
            case "unsignedLong" -> { FUnsignedLong f = new FUnsignedLong(); f.v = new BigInteger(t); yield f; }
            case "decimal" -> { FDecimal f = new FDecimal(); f.v = new BigDecimal(t); yield f; }
            case "int" -> { FInt f = new FInt(); f.v = Integer.parseInt(t); yield f; }
            case "short" -> { FShort f = new FShort(); f.v = Short.parseShort(t); yield f; }
            case "byte" -> { FByte f = new FByte(); f.v = Byte.parseByte(t); yield f; }
            case "unsignedInt" -> { FUnsignedInt f = new FUnsignedInt(); f.v = Long.parseLong(t); yield f; }
            case "unsignedShort" -> { FUnsignedShort f = new FUnsignedShort(); f.v = Integer.parseInt(t); yield f; }
            case "unsignedByte" -> { FUnsignedByte f = new FUnsignedByte(); f.v = Short.parseShort(t); yield f; }
            case "counts" -> { FCounts f = new FCounts(); f.v = new ArrayList<>(); for (JsonNode n : value) f.v.add(new BigInteger(n.asText())); yield f; }
            case "amount" -> { FAmount f = new FAmount(); f.v = new Amount(); f.v.value = new BigDecimal(value.path("value").asText()); yield f; }
            default -> throw new IllegalStateException("no binding type for field " + field);
        };
    }

    static boolean refusedByForm(Exception e) {
        for (Throwable t = e; t != null; t = t.getCause()) {
            if (t.getMessage() != null && t.getMessage().contains("[json-number-form]")) return true;
        }
        return false;
    }

    static List<String> bindingFailures(File vectors) throws Exception {
        JsonNode doc = PLAIN.readTree(new File(vectors, "codec-vectors-json-numbers.json"));
        List<String> failures = new ArrayList<>();
        int checked = 0;
        for (JsonNode c : doc.path("readCases")) {
            checked++;
            String id = "binding read " + c.path("id").asText();
            boolean expectError = c.path("expectError").asBoolean(false);
            Object got;
            try {
                got = readAs(c.path("field").asText(), c.path("json").asText());
            } catch (JsonProcessingException e) {
                if (!expectError) failures.add(id + ": refused: " + e.getOriginalMessage());
                else if (!refusedByForm(e)) failures.add(id + ": refused without [json-number-form]: " + e.getOriginalMessage());
                continue;
            }
            if (expectError) failures.add(id + ": expected a refusal, got " + lexical(got));
            else if (!PLAIN.valueToTree(lexical(got)).equals(c.path("expected"))) {
                failures.add(id + ": expected " + c.path("expected") + ", got " + lexical(got));
            }
        }
        for (JsonNode c : doc.path("writeCases")) {
            checked++;
            String id = "binding write " + c.path("id").asText();
            JsonNode want = c.has("json") ? PLAIN.readTree(c.path("json").asText()) : null;
            JsonNode written;
            try {
                written = PLAIN.valueToTree(holderOf(c.path("field").asText(), c.path("value"))).path("v");
            } catch (IllegalArgumentException e) {
                if (want != null) failures.add(id + ": refused: " + e.getMessage());
                continue;
            }
            if (want == null) failures.add(id + ": expected a refusal, wrote " + written);
            else if (!PLAIN.writeValueAsString(written).equals(PLAIN.writeValueAsString(want))) {
                failures.add(id + ": expected " + want + ", got " + written);
            }
        }
        checked++;
        Restriction r = PLAIN.readValue("{\"cardinality\": \"1\"}", Restriction.class);
        if (!BigInteger.ONE.equals(r.cardinality)) failures.add("binding cardinality: \"1\" read as " + r.cardinality);
        failures.forEach(f -> System.out.println("  FAIL " + f));
        System.out.println("codec-vectors-json-numbers.json (Jackson binding): " + (checked - failures.size()) + "/" + checked + " pass");
        return failures;
    }
}
