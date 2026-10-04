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
Codec.typeMatches(node, classUri, schema); // a generated type guard (also takes a KanonakNode)
Codec.enumMember(schema, ref);            // -> EnumMemberMatch(enumType, uri, member), or null
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

An object property's value is either a reference (`{"$ref": uri}`) or an
embedded value (0.2.0): a map with no `$id`, an optional `$name` (the authored
dict-key — hash-relevant), an optional `$type` (emits a type statement when
present), and schema-mapped fields. Without `$type`, fields map via the
containing property's `range` (inference only — no type statement is emitted).
A reference must address a named resource (`publisher/package[@version]/name`):
a `$ref` containing a URI fragment (`#`) is rejected, at any depth, by
canonicalization, `serialize` and `deserialize`, with a message ending in
`[fragment-reference]` (runtime#6).

### Reading an earlier compatible version (0.6.1)

A node is typed with the class version its producer resolved; a codec built
from a later COMPATIBLE version of the same package still reads it.
`deserialize` resolves `$type` by the exact key first, then by the same
publisher/package/name at the highest schema version for which
`Coordinate.isReadableBy(written, schemaVersion)` (kanonak-canonical) holds,
and the node keeps its WRITTEN `$type`. A rejection message ends in a
bracketed kind: `[unknown-type]`, `[newer-version]`, `[other-major]` or
`[other-minor-line]`. `typeMatches` resolves each of the node's types
(`$types`, else `$type`) the same way and matches the class itself or any
entry of the optional `CodecClass.ancestors()` (every superclass, by versioned
URI; canonicalization never reads it, and a schema without it still parses);
unreadable types never match. `enumMember` resolves a `$ref` exactly, then to
the same member at a version that can read it. Hashing stays EXACT-version
only: a node or embedded value whose `$type` is only compatibly present is
rejected with `[hash-needs-exact-version]`, since its predicates would carry
the schema's version instead of the producer's.

## Typed surface (0.3.0)

The SDK-facing typed surface a generator emits against: classes extend
`KanonakNode` (the `$`-envelope as data — `id`, `typeUri`, `name`,
`packageContentHash`, `packageVersion`, plus `extra` for open-world
assertions keyed by predicate URI), annotate every property field with
`@WireName("localName")`, and type object properties as `Ref<T>` /
`List<Ref<T>>` — EXACTLY ONE of a reference (`Ref.to(uri)` / `Ref.to(node)`,
resolved through the target's id) or an embedded value (`Ref.embed(value)` /
`Ref.embed(value, name)` — the name rides `$name` and is hash-relevant). The
reference-vs-embedded choice is authorial and hash-relevant, never inferred;
range-derived typing of an untyped embedded is inference only.

```java
TypedNodes.toNode(typed, schema);  // typed instance -> node (the Map contract)
```

`toNode` projects the instance to its wire map reflectively (JDK-only — no
JSON library) and splits it back through `Codec.deserialize`, so the typed
path and the dictionary path are one contract, not two.

## Conformance

JDK-only — mirrors the `kanonak-canonical` port: compile the canonical sources,
the codec sources, and both conformance drivers together on one `javac`
classpath, then run both against the shared vectors.

```bash
javac -d out \
  ../../kanonak-canonical/java/src/main/java/org/kanonak/canonical/*.java \
  src/main/java/org/kanonak/codec/*.java \
  conformance/Conformance.java conformance/TypedConformance.java
java -cp out Conformance        # runs every ../vectors/codec-vectors*.json file
                                # (base, embedded, types, references, enums,
                                # compat)
java -cp out TypedConformance   # the typed surface (KanonakNode / Ref /
                                # TypedNodes) against the same vectors
```

Expected basic-case hash:
`sha256:6ed4e664dbaf7d3331d71af297f48da23994af34d081a86f555cb34706de2913`.
