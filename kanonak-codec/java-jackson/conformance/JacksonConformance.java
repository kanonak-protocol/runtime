import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.TreeSet;

import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.json.JsonMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.kanonak.codec.KanonakNode;
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
}
