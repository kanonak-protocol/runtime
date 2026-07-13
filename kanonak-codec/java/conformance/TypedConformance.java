import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import org.kanonak.codec.Codec;
import org.kanonak.codec.CodecSchema;
import org.kanonak.codec.KanonakNode;
import org.kanonak.codec.PackageContext;
import org.kanonak.codec.Ref;
import org.kanonak.codec.TypedNodes;
import org.kanonak.codec.WireName;

/**
 * Typed-surface conformance: hand-written GENERATED-STYLE classes for the
 * embedded-vectors probe schema, driven through the KanonakNode / Ref / TypedNodes
 * binding, asserted against the SAME golden vectors the node contract is gated by
 * (codec-vectors.json + codec-vectors-embedded.json). This file is also the
 * executable spec for what an SDK generator must emit: classes extend
 * {@link KanonakNode}, object properties are {@code Ref<T>}/{@code List<Ref<T>>},
 * wire names ride {@link WireName}. Compiled and run alongside
 * {@link Conformance} (whose vector parsing it reuses).
 */
public final class TypedConformance {
    static final String SCHEMA = "probe.example.com/schema@1.0.0";
    static final String DATA = "probe.example.com/data@1.0.0";

    // -- Generated-style model for probe.example.com/schema@1.0.0 -------------

    static final class Order extends KanonakNode {
        @WireName("note") String note;
        @WireName("items") List<Ref<LineItem>> items;
        @WireName("customer") List<Ref<Customer>> customer;
    }

    /**
     * Same $type, single-valued customer — the wire/hash contract is carried by
     * $type + @WireName, not the Java class name, so a second fixture shape
     * exercises the bare (non-list) embedded form.
     */
    static final class OrderSingleCustomer extends KanonakNode {
        @WireName("note") String note;
        @WireName("customer") Ref<Customer> customer;
    }

    static final class LineItem extends KanonakNode {
        @WireName("sku") String sku;
        @WireName("qty") Long qty;
    }

    static final class Customer extends KanonakNode {
        @WireName("name") String name;
        @WireName("address") List<Ref<Address>> address;
    }

    static final class Address extends KanonakNode {
        @WireName("city") String city;
    }

    static final class Person extends KanonakNode {
        @WireName("name") String name;
    }

    // -- Generated-style model for the $types vectors (0.4.0, runtime#10) -----
    // The multi-typed set rides the model via KanonakNode.setTypes ($types
    // envelope only — deliberately no unprefixed wire name, because an ontology
    // can model a property literally named "types").

    static final class DefResource extends KanonakNode {
        @WireName("note") String note;
    }

    static final class Bundle extends KanonakNode {
        @WireName("parts") List<Ref<PartDef>> parts;
    }

    /** Same $type, single-valued parts — exercises the bare embedded form. */
    static final class BundleSinglePart extends KanonakNode {
        @WireName("parts") Ref<PartDef> parts;
    }

    static final class PartDef extends KanonakNode {
        @WireName("size") Long size;
    }

    static final class Account extends KanonakNode {
        @WireName("accountCode") String accountCode;
        @WireName("seats") Long seats;
        @WireName("rate") Double rate;
        @WireName("active") Boolean active;
        @WireName("owner") Ref<Person> owner;
        @WireName("tags") List<String> tags;
    }

    static int passed;
    static int failed;

    public static void main(String[] args) throws Exception {
        Map<String, Object> embedded = load("../vectors/codec-vectors-embedded.json");
        Map<String, Object> basic = load("../vectors/codec-vectors.json");
        CodecSchema embSchema = Conformance.parseSchema(Conformance.asMap(embedded.get("schema")));
        CodecSchema basicSchema = Conformance.parseSchema(Conformance.asMap(basic.get("schema")));

        // Each typed fixture must reproduce the golden expected values of the
        // NAMED vector case — the typed path and the node path are one contract.

        check(embedded, "embedded-named-in-list", embSchema, List.of(
            order(o -> {
                o.note = "A";
                o.items = List.of(Ref.embed(lineItem("X", 1L), "first"));
            })));

        check(embedded, "embedded-unnamed-positional", embSchema, List.of(
            order(o -> {
                o.note = "A";
                o.items = List.of(Ref.embed(lineItem("X", 1L)));
            })));

        check(embedded, "embedded-explicit-type", embSchema, List.of(
            order(o -> {
                o.note = "A";
                LineItem item = lineItem("X", 1L);
                item.setTypeUri(SCHEMA + "/LineItem");
                o.items = List.of(Ref.embed(item, "first"));
            })));

        check(embedded, "embedded-list-order", embSchema, List.of(
            order(o -> o.items = List.of(
                Ref.embed(lineItem("X", 1L), "a"),
                Ref.embed(lineItem("Y", 2L), "b")))));

        check(embedded, "embedded-nested", embSchema, List.of(
            order(o -> {
                Address home = new Address();
                home.city = "Austin";
                Customer ada = new Customer();
                ada.name = "Ada";
                ada.address = List.of(Ref.embed(home, "home"));
                o.customer = List.of(Ref.embed(ada, "cust"));
            })));

        {
            OrderSingleCustomer o = new OrderSingleCustomer();
            o.setId(DATA + "/o1");
            o.setTypeUri(SCHEMA + "/Order");
            Customer ada = new Customer();
            ada.name = "Ada";
            o.customer = Ref.embed(ada, "cust");
            check(embedded, "single-embedded-bare", embSchema, List.of(o));
        }

        check(embedded, "empty-list-emits-nothing", embSchema, List.of(
            order(o -> {
                o.note = "A";
                o.items = new ArrayList<>();
            })));

        // The 0.1.0 basic case through the typed path: references + scalar list —
        // once with a URI reference, once resolved through the target instance.
        check(basic, "basic-scalars-ref-list (Ref.to uri)", "basic-scalars-ref-list",
            basicSchema, List.of(alice(), account(Ref.to(DATA + "/p1"))));

        Person alice = alice();
        check(basic, "basic-scalars-ref-list (Ref.to instance)", "basic-scalars-ref-list",
            basicSchema, List.of(alice, account(Ref.to(alice))));

        // The 0.4.0 $types cases (runtime#10) through the typed path.
        Map<String, Object> types = load("../vectors/codec-vectors-types.json");
        CodecSchema typesSchema = Conformance.parseSchema(Conformance.asMap(types.get("schema")));

        {
            DefResource def = new DefResource();
            def.setId(DATA + "/w1");
            def.setTypeUri(SCHEMA + "/ClassDef");
            def.setTypes(List.of(SCHEMA + "/AnnotatedDef", SCHEMA + "/ClassDef"));
            def.note = "A";
            check(types, "covered-redundant-set", typesSchema, List.of(def));
        }

        {
            BundleSinglePart b = new BundleSinglePart();
            b.setId(DATA + "/b1");
            b.setTypeUri(SCHEMA + "/Bundle");
            b.parts = Ref.embed(partDef(2L, true), "first");
            check(types, "embedded-multi-typed-named", typesSchema, List.of(b));
        }

        {
            Bundle b = new Bundle();
            b.setId(DATA + "/b1");
            b.setTypeUri(SCHEMA + "/Bundle");
            b.parts = List.of(
                Ref.embed(partDef(1L, true), "a"),
                Ref.embed(partDef(2L, false), "b"));
            check(types, "types-in-list-items", typesSchema, List.of(b));
        }

        System.out.println("\n" + passed + " passed, " + failed + " failed");
        System.exit(failed == 0 ? 0 : 1);
    }

    // -- Fixture builders ------------------------------------------------------

    interface OrderInit {
        void init(Order o);
    }

    static Order order(OrderInit init) {
        Order o = new Order();
        o.setId(DATA + "/o1");
        o.setTypeUri(SCHEMA + "/Order");
        init.init(o);
        return o;
    }

    static LineItem lineItem(String sku, Long qty) {
        LineItem item = new LineItem();
        item.sku = sku;
        item.qty = qty;
        return item;
    }

    static PartDef partDef(Long size, boolean multiTyped) {
        PartDef part = new PartDef();
        part.setTypeUri(SCHEMA + "/PartDef");
        if (multiTyped) {
            part.setTypes(List.of(SCHEMA + "/PartDef", SCHEMA + "/SealedDef"));
        }
        part.size = size;
        return part;
    }

    static Person alice() {
        Person p = new Person();
        p.setId(DATA + "/p1");
        p.setTypeUri(SCHEMA + "/Person");
        p.name = "Alice";
        return p;
    }

    static Account account(Ref<Person> owner) {
        Account a = new Account();
        a.setId(DATA + "/a1");
        a.setTypeUri(SCHEMA + "/Account");
        a.accountCode = "paul";
        a.seats = 5L;
        a.rate = 1.5; // Double: lexical() renders "1.5" — matches the vector token
        a.active = Boolean.TRUE;
        a.owner = owner;
        a.tags = List.of("x", "y");
        return a;
    }

    // -- Harness ----------------------------------------------------------------

    static void check(Map<String, Object> vectors, String caseId, CodecSchema schema,
                      List<? extends KanonakNode> typed) {
        check(vectors, caseId, caseId, schema, typed);
    }

    static void check(Map<String, Object> vectors, String label, String caseId,
                      CodecSchema schema, List<? extends KanonakNode> typed) {
        Map<String, Object> theCase = null;
        for (Object co : Conformance.asList(vectors.get("cases"))) {
            Map<String, Object> c = Conformance.asMap(co);
            if (caseId.equals(c.get("id"))) {
                theCase = c;
                break;
            }
        }
        if (theCase == null) {
            fail(label, "vector case not found");
            return;
        }
        PackageContext pkg = Conformance.parsePkg(Conformance.asMap(theCase.get("pkg")));
        String expForm = (String) theCase.get("expectedCanonicalForm");
        String expHash = (String) theCase.get("expectedHash");

        try {
            List<Map<String, Object>> nodes = new ArrayList<>(typed.size());
            for (KanonakNode t : typed) {
                nodes.add(TypedNodes.toNode(t, schema));
            }
            String form = Codec.canonicalForm(nodes, schema, pkg);
            String hash = Codec.contentHash(nodes, schema, pkg);
            if (!form.equals(expForm)) {
                fail(label, "canonical form mismatch\n  exp: " + expForm + "\n  got: " + form);
            } else if (!hash.equals(expHash)) {
                fail(label, "hash expected " + expHash + " got " + hash);
            } else {
                passed++;
                System.out.println("PASS  " + label);
            }
        } catch (Exception ex) {
            fail(label, ex.getClass().getSimpleName() + ": " + ex.getMessage());
        }
    }

    static void fail(String label, String message) {
        failed++;
        System.out.println("FAIL  " + label + ": " + message);
    }

    static Map<String, Object> load(String path) throws Exception {
        return Conformance.asMap(Conformance.Json.parse(
            Files.readString(Paths.get(path), StandardCharsets.UTF_8)));
    }
}
