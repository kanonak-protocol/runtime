# kanonak-codec (Java)

The generic, ontology-independent **codec runtime** — the Java port. Given a
`CodecSchema` (the per-package metadata a generated typed SDK embeds) and a set
of typed nodes, it builds the language-neutral canonical input model and
content-addresses it via [`kanonak-canonical`](../../kanonak-canonical/java), so
the hash byte-matches the `kanonak hash` CLI and every other language port.

- Maven coordinates: **`org.kanonak:kanonak-codec`**
- Package: `org.kanonak.codec`
- Java 17, JDK-only (depends only on `org.kanonak:kanonak-canonical`)

## API

A node is a `Map<String, Object>`: the `$`-envelope (`$type`, `$id`, optional
`$extra`) plus alias-collapsed local-name fields
(`String` / `Boolean` / `Number` / `List` / `{"$ref": uri}` as a `Map`).

```java
Codec.buildPackage(nodes, schema, pkg);   // -> CanonicalForm.Package
Codec.canonicalForm(nodes, schema, pkg);  // the {subjects:[...]} JSON
Codec.contentHash(nodes, schema, pkg);    // "sha256:..." — matches `kanonak hash`
Codec.serialize(node);                    // node -> normalized-JSON wire Map
Codec.deserialize(json, schema);          // normalized-JSON Map -> node
```

`Codec.buildPackage` synthesizes the `rdf:type` triple for each node and the
package-wrapper subject (`<publisher>/<packageName>@<version>/<packageName>`,
with the optional package label and the `Package` type triple).

`serialize` projects the modeled fields (dropping nulls) and spreads `$extra`
entries as sibling fields **after** them (a modeled field wins a name collision;
no `$extra` key appears on the wire). `deserialize` does the inverse split:
`$`-prefixed keys and keys modeled on `$type` stay top-level, everything else
goes under `$extra`.

Numeric scalars may be supplied as a plain `Number` or as a `JsonNumber` (which
retains the exact source token, so the lexical form has no locale or
trailing-zero/scientific artifacts).

Embedded object values (a node inline rather than `{"$ref": ...}`) are not yet
supported and throw loudly.

## Conformance

JDK-only — mirrors the `kanonak-canonical` port: compile the canonical sources,
the codec sources, and the conformance driver together on one `javac` classpath,
then run against the shared vectors.

```bash
javac -d out \
  ../../kanonak-canonical/java/src/main/java/org/kanonak/canonical/*.java \
  src/main/java/org/kanonak/codec/*.java \
  conformance/Conformance.java
java -cp out Conformance ../vectors/codec-vectors.json
```

Expected basic-case hash:
`sha256:6ed4e664dbaf7d3331d71af297f48da23994af34d081a86f555cb34706de2913`.
